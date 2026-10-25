import { useStore } from "zustand";
import { displayName } from "../domain/names";
import { projectName } from "../domain/zones";
import type { KitiStore } from "../state/store";
import { ago, STATUS_LABEL } from "./labels";
import s from "./panels.module.css";

type Props = { store: KitiStore; now: () => number };

/**
 * Qué es y en qué está la oficina bajo el ratón, sin abrir su terminal (ADR 0009): título de la
 * conversación, actividad actual y último pedido. Una apagada invita a retomarla.
 */
export function OfficeTooltip({ store, now }: Props) {
  const hover = useStore(store, (st) => st.hover);
  const agent = useStore(store, (st) => (st.hover ? st.agents.agents.get(st.hover.id) : undefined));
  const closed = useStore(store, (st) => (st.hover ? st.dormant.get(st.hover.id) : undefined));
  // La seleccionada ya tiene su panel con lo mismo: el tooltip solo lo taparía.
  const selected = useStore(
    store,
    (st) => st.hover !== undefined && st.hover.id === st.view.selectedId,
  );
  if (!hover || selected || (!agent && !closed)) return null;
  const style = { left: hover.x + 14, top: hover.y + 14 };
  if (agent) {
    return (
      <div role="tooltip" className={s.tooltip} style={style}>
        <strong>{agent.title ?? displayName(agent.cwd)}</strong>
        <div className={s.tipMeta}>
          {projectName(agent.project || agent.cwd)} · {agent.provider} ·{" "}
          {STATUS_LABEL[agent.status.kind]}
        </div>
        {agent.activity && <div>▶ {agent.activity}</div>}
        {agent.lastPrompt && <div className={s.tipPrompt}>«{agent.lastPrompt}»</div>}
      </div>
    );
  }
  if (!closed) return null;
  return (
    <div role="tooltip" className={s.tooltip} style={style}>
      <strong>{closed.title ?? displayName(closed.cwd)}</strong>
      <div className={s.tipMeta}>
        {projectName(closed.project || closed.cwd)} · cerrada {ago(now() - closed.updatedAt)}
      </div>
      {closed.lastPrompt && <div className={s.tipPrompt}>«{closed.lastPrompt}»</div>}
      <div className={s.tipMeta}>Clic para retomarla</div>
    </div>
  );
}
