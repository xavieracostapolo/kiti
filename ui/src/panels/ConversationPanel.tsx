import { useState } from "react";
import { useStore } from "zustand";
import { displayName } from "../domain/names";
import { projectName } from "../domain/zones";
import type { KitiStore } from "../state/store";
import type { WorldHandle } from "../world/World";
import { ago } from "./labels";
import s from "./panels.module.css";

type Props = {
  store: KitiStore;
  world: () => WorldHandle | undefined;
  now: () => number;
  /** Solo en la app nativa: `claude --resume` en una terminal de Kiti. */
  onResume?: (sessionId: string) => Promise<void>;
};

/** Detalle de una oficina apagada: una conversación cerrada que se puede retomar (ADR 0009). */
export function ConversationPanel({ store, world, now, onResume }: Props) {
  const selected = useStore(store, (st) => st.view.selectedId);
  const conv = useStore(store, (st) => (selected ? st.dormant.get(selected) : undefined));
  const terminalOpen = useStore(store, (st) => st.openTerminalId !== undefined);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>();
  if (!selected || !conv || terminalOpen) return null;

  const resume = async () => {
    if (!onResume) return;
    setBusy(true);
    setError(undefined);
    try {
      await onResume(conv.sessionId);
    } catch (e) {
      const message = (e as { message?: string }).message;
      setError(message ?? "No se pudo retomar la conversación");
    } finally {
      setBusy(false);
    }
  };

  return (
    <aside className={s.agentPanel} aria-label="Conversación cerrada">
      <h2 className={s.agentTitle}>
        <span className={s.dot} style={{ background: "#4a5266" }} />
        {conv.title ?? displayName(conv.cwd)}
      </h2>
      <dl className={s.fields}>
        <dt>proyecto</dt>
        <dd>{projectName(conv.project || conv.cwd)}</dd>
        <dt>último pedido</dt>
        <dd>{conv.lastPrompt ?? "—"}</dd>
        <dt>cerrada</dt>
        <dd>{ago(now() - conv.updatedAt)}</dd>
        <dt>cwd</dt>
        <dd>{conv.cwd}</dd>
      </dl>
      <p className={s.notice}>
        Conversación cerrada. Al retomarla, Claude la abre con todo su contexto en una terminal de
        Kiti.
      </p>
      {error && (
        <p className={s.notice} role="alert">
          {error}
        </p>
      )}
      <div className={s.panelActions}>
        {onResume && (
          <button type="button" className={s.button} disabled={busy} onClick={resume}>
            {busy ? "Retomando…" : "Retomar"}
          </button>
        )}
        <button type="button" className={s.button} onClick={() => world()?.focus(selected)}>
          Enfocar
        </button>
      </div>
    </aside>
  );
}
