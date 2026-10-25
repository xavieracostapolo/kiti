import { useStore } from "zustand";
import { displayName } from "../domain/names";
import { projectName } from "../domain/zones";
import type { KitiStore } from "../state/store";
import { STATUS_COLOR } from "../world/palette";
import type { WorldHandle } from "../world/World";
import { ago, REASON_LABEL, STATUS_LABEL } from "./labels";
import s from "./panels.module.css";

const hex = (n: number) => `#${n.toString(16).padStart(6, "0")}`;

type Props = {
  store: KitiStore;
  world: () => WorldHandle | undefined;
  now: () => number;
  /** Hay terminales (app nativa): se ofrece abrir la del agente o se explica por qué no. */
  terminalsAvailable?: boolean;
};

/** Detalle del agente seleccionado. Solo se re-renderiza cuando cambia ese agente. */
export function AgentPanel({ store, world, now, terminalsAvailable = false }: Props) {
  const agent = useStore(store, (st) =>
    st.view.selectedId ? st.agents.agents.get(st.view.selectedId) : undefined,
  );
  const terminal = useStore(store, (st) => (agent ? st.terminals.get(agent.pid) : undefined));
  // Con la terminal abierta, su cabecera ya dice qué agente es: el panel no tapa oficinas.
  const terminalOpen = useStore(store, (st) => st.openTerminalId !== undefined);
  if (!agent || terminalOpen) return null;
  return (
    <aside className={s.agentPanel} aria-label="Agente seleccionado">
      <h2 className={s.agentTitle}>
        <span className={s.dot} style={{ background: hex(STATUS_COLOR[agent.status.kind]) }} />
        {agent.title ?? displayName(agent.cwd)}
      </h2>
      <dl className={s.fields}>
        {agent.project && agent.project !== agent.cwd && (
          <>
            <dt>proyecto</dt>
            <dd>{projectName(agent.project)}</dd>
          </>
        )}
        {agent.activity && (
          <>
            <dt>ahora</dt>
            <dd>{agent.activity}</dd>
          </>
        )}
        {agent.lastPrompt && (
          <>
            <dt>último pedido</dt>
            <dd>{agent.lastPrompt}</dd>
          </>
        )}
        <dt>proveedor</dt>
        <dd>{agent.provider}</dd>
        <dt>estado</dt>
        <dd>
          {STATUS_LABEL[agent.status.kind]} · {ago(now() - agent.status.since)}
        </dd>
        <dt>motivo</dt>
        <dd>{REASON_LABEL[agent.status.reason]}</dd>
        <dt>herramienta</dt>
        <dd>{agent.lastTool ?? "—"}</dd>
        <dt>último evento</dt>
        <dd>{ago(now() - agent.lastEventAt)}</dd>
        <dt>pid</dt>
        <dd>{agent.pid}</dd>
        <dt>cwd</dt>
        <dd>{agent.cwd}</dd>
      </dl>
      {terminalsAvailable && !terminal && (
        <p className={s.notice}>
          Abierto fuera de Kiti: su terminal no se puede mostrar aquí. Lanza agentes con «Nuevo
          agente» para tenerlos dentro.
        </p>
      )}
      <div className={s.panelActions}>
        {terminal && (
          <button
            type="button"
            className={s.button}
            onClick={() => store.getState().openTerminal(terminal.id)}
          >
            Abrir terminal
          </button>
        )}
        <button type="button" className={s.button} onClick={() => world()?.focus(agent.id)}>
          Enfocar
        </button>
        <button
          type="button"
          className={`${s.button} ${s.danger}`}
          onClick={() => store.getState().requestKill(agent.id)}
        >
          Cerrar oficina
        </button>
      </div>
    </aside>
  );
}
