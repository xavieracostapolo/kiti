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
  /** Solo en la app nativa: no vuelve a salir en Kiti (no borra la conversación de Claude). */
  onArchive?: (sessionId: string) => Promise<void>;
};

/** Detalle de una oficina apagada: una conversación cerrada que se puede retomar (ADR 0009). */
export function ConversationPanel(props: Props) {
  // Con otra oficina seleccionada el panel empieza de cero (sin confirmación ni error colgados).
  const selected = useStore(props.store, (st) => st.view.selectedId);
  return <Details key={selected ?? ""} {...props} />;
}

function Details({ store, world, now, onResume, onArchive }: Props) {
  const selected = useStore(store, (st) => st.view.selectedId);
  const conv = useStore(store, (st) => (selected ? st.dormant.get(selected) : undefined));
  const terminalOpen = useStore(store, (st) => st.openTerminalId !== undefined);
  const [busy, setBusy] = useState(false);
  const [confirmArchive, setConfirmArchive] = useState(false);
  const [error, setError] = useState<string | undefined>();
  if (!selected || !conv || terminalOpen) return null;

  const run = async (action: (id: string) => Promise<void>, failure: string) => {
    setBusy(true);
    setError(undefined);
    try {
      await action(conv.sessionId);
    } catch (e) {
      setError((e as { message?: string }).message ?? failure);
    } finally {
      setBusy(false);
    }
  };
  const resume = () => onResume && run(onResume, "No se pudo retomar la conversación");
  const archive = () => onArchive && run(onArchive, "No se pudo archivar la conversación");

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
      {confirmArchive && (
        <p className={s.notice} role="alert">
          Dejará de salir en Kiti. La conversación sigue en Claude: puedes retomarla con{" "}
          <code>claude --resume</code>.
        </p>
      )}
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
        {onArchive &&
          (confirmArchive ? (
            <button
              type="button"
              className={`${s.button} ${s.danger}`}
              disabled={busy}
              onClick={archive}
            >
              Sí, archivar
            </button>
          ) : (
            <button type="button" className={s.button} onClick={() => setConfirmArchive(true)}>
              Archivar
            </button>
          ))}
      </div>
    </aside>
  );
}
