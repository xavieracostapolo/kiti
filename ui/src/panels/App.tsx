import { Profiler, useCallback, useRef } from "react";
import type { Params } from "../dev/params";
import type { KitiStore } from "../state/store";
import type { Metrics } from "../telemetry/metrics";
import type { Transport } from "../transport/Transport";
import type { WorldHandle } from "../world/World";
import { AgentPanel } from "./AgentPanel";
import { CloseOfficeDialog } from "./CloseOfficeDialog";
import { DebugPanel } from "./DebugPanel";
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

  return (
    <Profiler id="panels" onRender={() => metrics.reactCommit()}>
      <WorldHost store={store} metrics={metrics} params={params} onWorld={handleWorld} />
      <TopBar store={store} />
      <AgentPanel store={store} world={() => world.current} now={now} />
      {transport.killConfirmation === "in-app" && (
        <CloseOfficeDialog store={store} killAgent={killAgent} />
      )}
      <DebugPanel metrics={metrics} store={store} transport={transport} />
    </Profiler>
  );
}
