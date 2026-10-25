import { useStore } from "zustand";
import { useShallow } from "zustand/react/shallow";
import type { StatusKind } from "../protocol";
import { countsByKind } from "../state/agents";
import type { KitiStore } from "../state/store";
import { STATUS_COLOR } from "../world/palette";
import { STATUS_LABEL as LABEL } from "./labels";
import s from "./panels.module.css";

const ORDER: StatusKind[] = ["working", "waiting", "idle", "error", "dead"];

const hex = (n: number) => `#${n.toString(16).padStart(6, "0")}`;

export function TopBar({ store }: { store: KitiStore }) {
  const counts = useStore(
    store,
    useShallow((st) => countsByKind(st.agents.agents)),
  );
  const simulated = useStore(store, (st) => st.source === "sim");
  const total = ORDER.reduce((sum, k) => sum + counts[k], 0);
  return (
    <header className={s.topbar}>
      <span className={s.brand}>KITI</span>
      {simulated && (
        <span className={s.simBadge} title="Agentes generados por el simulador, no procesos reales">
          SIMULADO
        </span>
      )}
      <span>{total} agentes</span>
      <span className={s.counts}>
        {ORDER.map((k) => (
          <span key={k} className={s.count} data-testid={`count-${k}`}>
            <span className={s.dot} style={{ background: hex(STATUS_COLOR[k]) }} />
            {counts[k]} {LABEL[k]}
          </span>
        ))}
      </span>
      <span className={s.spacer} />
      <span className={s.hint}>
        arrastra · rueda · ⌘+rueda zoom · doble clic enfoca · 0 ver todo
      </span>
    </header>
  );
}
