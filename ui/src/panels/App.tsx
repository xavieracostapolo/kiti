import { Profiler, useCallback, useRef } from "react";
import type { Params } from "../dev/params";
import type { Provider } from "../protocol";
import type { KitiStore } from "../state/store";
import type { Metrics } from "../telemetry/metrics";
import type { Transport } from "../transport/Transport";
import type { WorldHandle } from "../world/World";
import { AgentPanel } from "./AgentPanel";
import { CloseOfficeDialog } from "./CloseOfficeDialog";
import { ConversationPanel } from "./ConversationPanel";
import { DebugPanel } from "./DebugPanel";
import { NativeCloseOffice } from "./NativeCloseOffice";
import { OfficeTooltip } from "./OfficeTooltip";
import { TerminalPanel } from "./TerminalPanel";
import { TopBar } from "./TopBar";
import { WorldHost } from "./WorldHost";

type Props = {
  store: KitiStore;
  transport: Transport;
  metrics: Metrics;
  params: Params;
  now: () => number;
  onWorld: (world: WorldHandle | undefined) => void;
};

export function App({ store, transport, metrics, params, now, onWorld }: Props) {
  const world = useRef<WorldHandle | undefined>(undefined);
  const handleWorld = useCallback(
    (w: WorldHandle | undefined) => {
      world.current = w;
      onWorld(w);
    },
    [onWorld],
  );
  const killAgent = useCallback((id: string) => transport.killAgent(id), [transport]);
  const terminals = transport.terminals;
  const conversations = transport.conversations;

  const launch = useCallback(
    async (provider: Provider) => {
      if (!terminals) return;
      try {
        const info = await terminals.launch(provider);
        if (!info) return; // canceló el selector de carpeta
        store.getState().setTerminals(await terminals.list());
        // Oficina primero: el mundo la enfoca y la selecciona en cuanto aparece (≤ 3 s).
        store.getState().setPendingLaunch(info.pid);
      } catch (e) {
        console.error(`No se pudo lanzar ${provider}: ${JSON.stringify(e)}`);
      }
    },
    [store, terminals],
  );

  /** Retoma una conversación cerrada (ADR 0009): misma secuencia que lanzar, oficina primero. */
  const resume = useCallback(
    async (sessionId: string) => {
      if (!conversations) return;
      const info = await conversations.resume(sessionId);
      const st = store.getState();
      st.setConversations(st.conversations.filter((c) => c.sessionId !== sessionId));
      if (terminals) st.setTerminals(await terminals.list());
      st.setPendingLaunch(info.pid);
    },
    [store, conversations, terminals],
  );

  /** Archiva una conversación cerrada: sale del mapa ahora y no vuelve (ADR 0009). */
  const archive = useCallback(
    async (sessionId: string) => {
      if (!conversations) return;
      await conversations.hide(sessionId);
      const st = store.getState();
      st.setConversations(st.conversations.filter((c) => c.sessionId !== sessionId));
    },
    [store, conversations],
  );

  return (
    <Profiler id="panels" onRender={() => metrics.reactCommit()}>
      <WorldHost store={store} metrics={metrics} params={params} onWorld={handleWorld} now={now} />
      <TopBar store={store} now={now} {...(terminals ? { onLaunch: launch } : {})} />
      <AgentPanel
        store={store}
        world={() => world.current}
        now={now}
        terminalsAvailable={terminals !== undefined}
        {...(terminals ? { onFocusExternal: (id: string) => terminals.focusExternal(id) } : {})}
      />
      <ConversationPanel
        store={store}
        world={() => world.current}
        now={now}
        {...(conversations ? { onResume: resume, onArchive: archive } : {})}
      />
      {terminals && <TerminalPanel store={store} api={terminals} />}
      <OfficeTooltip store={store} now={now} />
      {transport.killConfirmation === "in-app" ? (
        <CloseOfficeDialog store={store} killAgent={killAgent} />
      ) : (
        <NativeCloseOffice store={store} killAgent={killAgent} />
      )}
      <DebugPanel metrics={metrics} store={store} transport={transport} />
    </Profiler>
  );
}
