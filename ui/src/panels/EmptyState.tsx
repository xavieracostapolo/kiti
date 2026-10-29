import { useStore } from "zustand";
import type { Provider } from "../protocol";
import type { KitiStore } from "../state/store";
import s from "./panels.module.css";

const CLIS: { provider: Provider; label: string }[] = [
  { provider: "claude", label: "Claude Code" },
  { provider: "codex", label: "Codex CLI" },
  { provider: "antigravity", label: "Antigravity CLI" },
];

type Props = {
  store: KitiStore;
  onLaunch?: (provider: Provider) => void;
};

/** Aviso central cuando no hay agentes encendidos ni conversaciones recientes en el mapa. */
export function EmptyState({ store, onLaunch }: Props) {
  const empty = useStore(
    store,
    (st) => st.agents.seq >= 0 && st.agents.agents.size === 0 && st.dormant.size === 0,
  );
  if (!empty) return null;

  return (
    <div className={s.emptyState} role="status">
      <h2 className={s.emptyTitle}>Sin agentes en ejecución</h2>
      <p className={s.emptyText}>
        {onLaunch
          ? "Abre un agente en tu terminal o lanza uno desde aquí."
          : "Abre Claude Code, Codex CLI o Antigravity CLI en tu terminal para ver su oficina."}
      </p>
      {onLaunch && (
        <div className={s.panelActions} style={{ justifyContent: "center" }}>
          {CLIS.map((cli) => (
            <button
              key={cli.provider}
              type="button"
              className={s.button}
              onClick={() => onLaunch(cli.provider)}
            >
              + {cli.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
