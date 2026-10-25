import { useStore } from "zustand";
import { displayName } from "../domain/names";
import type { KitiStore } from "../state/store";
import { STATUS_LABEL } from "./labels";
import s from "./panels.module.css";

/** Nombre y estado de la oficina bajo el ratón: útil en el zoom lejano, donde no hay nombres. */
export function OfficeTooltip({ store }: { store: KitiStore }) {
  const hover = useStore(store, (st) => st.hover);
  const agent = useStore(store, (st) => (st.hover ? st.agents.agents.get(st.hover.id) : undefined));
  if (!hover || !agent) return null;
  return (
    <div role="tooltip" className={s.tooltip} style={{ left: hover.x + 14, top: hover.y + 14 }}>
      <strong>{displayName(agent.cwd)}</strong> · {agent.provider} ·{" "}
      {STATUS_LABEL[agent.status.kind]}
    </div>
  );
}
