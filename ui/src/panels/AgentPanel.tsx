import { useStore } from "zustand";
import { displayName } from "../domain/names";
import type { KitiStore } from "../state/store";
import { STATUS_COLOR } from "../world/palette";
import type { WorldHandle } from "../world/World";
import { ago, REASON_LABEL, STATUS_LABEL } from "./labels";
import s from "./panels.module.css";

const hex = (n: number) => `#${n.toString(16).padStart(6, "0")}`;

type Props = { store: KitiStore; world: () => WorldHandle | undefined; now: () => number };

/** Detalle del agente seleccionado. Solo se re-renderiza cuando cambia ese agente. */
export function AgentPanel({ store, world, now }: Props) {
  const agent = useStore(store, (st) =>
    st.view.selectedId ? st.agents.agents.get(st.view.selectedId) : undefined,
  );
  if (!agent) return null;
  return (
    <aside className={s.agentPanel} aria-label="Agente seleccionado">
      <h2 className={s.agentTitle}>
        <span className={s.dot} style={{ background: hex(STATUS_COLOR[agent.status.kind]) }} />
        {displayName(agent.cwd)}
      </h2>
      <dl className={s.fields}>
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
      <div className={s.panelActions}>
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
