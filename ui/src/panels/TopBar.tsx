import { DropdownMenu } from "radix-ui";
import { useStore } from "zustand";
import { useShallow } from "zustand/react/shallow";
import type { Provider, StatusKind } from "../protocol";
import { countsByKind } from "../state/agents";
import type { KitiStore } from "../state/store";
import { STATUS_COLOR } from "../world/palette";
import { STATUS_LABEL as LABEL } from "./labels";
import s from "./panels.module.css";

const ORDER: StatusKind[] = ["working", "waiting", "idle", "error", "dead"];

const CLIS: { provider: Provider; label: string }[] = [
  { provider: "claude", label: "Claude Code" },
  { provider: "codex", label: "Codex CLI" },
  { provider: "gemini", label: "Gemini CLI" },
];

const hex = (n: number) => `#${n.toString(16).padStart(6, "0")}`;

type Props = {
  store: KitiStore;
  /** Solo con terminales (app nativa): lanza un agente del CLI elegido (ADR 0007). */
  onLaunch?: (provider: Provider) => void;
};

export function TopBar({ store, onLaunch }: Props) {
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
      {onLaunch && (
        <DropdownMenu.Root>
          <DropdownMenu.Trigger className={s.button}>+ Nuevo agente</DropdownMenu.Trigger>
          <DropdownMenu.Portal>
            <DropdownMenu.Content className={s.menu} align="end" sideOffset={6}>
              {CLIS.map((cli) => (
                <DropdownMenu.Item
                  key={cli.provider}
                  className={s.menuItem}
                  onSelect={() => onLaunch(cli.provider)}
                >
                  {cli.label}
                </DropdownMenu.Item>
              ))}
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
      )}
    </header>
  );
}
