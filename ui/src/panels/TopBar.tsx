import { DropdownMenu } from "radix-ui";
import { useEffect, useRef } from "react";
import { useStore } from "zustand";
import { useShallow } from "zustand/react/shallow";
import type { Provider, StatusKind } from "../protocol";
import { countsByKind } from "../state/agents";
import { type FilterKind, isActive } from "../state/filter";
import type { KitiStore } from "../state/store";
import { PAL, STATUS_COLOR } from "../world/palette";
import { STATUS_LABEL as LABEL } from "./labels";
import { MailChip } from "./MailPanel";
import s from "./panels.module.css";
import { QuotaBadge } from "./QuotaBadge";

const ORDER: StatusKind[] = ["working", "waiting", "idle", "error", "dead"];

const CLIS: { provider: Provider; label: string }[] = [
  { provider: "claude", label: "Claude Code" },
  { provider: "codex", label: "Codex CLI" },
  // Gemini CLI se retiró para las cuentas gratuitas: su sucesor es Antigravity CLI (ADR 0014).
  { provider: "antigravity", label: "Antigravity CLI" },
];

const hex = (n: number) => `#${n.toString(16).padStart(6, "0")}`;

const isTyping = (t: EventTarget | null) =>
  t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));

type Props = {
  store: KitiStore;
  /** Solo con terminales (app nativa): lanza un agente del CLI elegido (ADR 0007). */
  onLaunch?: (provider: Provider) => void;
  /** Reloj para "se reinicia en…" (el del simulador en modo sim). */
  now?: () => number;
};

/**
 * Los contadores también filtran (Hito 13): pulsar «2 esperando» atenúa el resto y encuadra esas
 * oficinas. El buscador (`/` lo enfoca, `Esc` lo limpia) busca por título, proyecto o carpeta.
 */
export function TopBar({ store, onLaunch, now }: Props) {
  const counts = useStore(
    store,
    useShallow((st) => countsByKind(st.agents.agents)),
  );
  const dormant = useStore(store, (st) => st.dormant.size);
  const filter = useStore(store, (st) => st.filter);
  const layoutMode = useStore(store, (st) => st.layoutMode);
  const simulated = useStore(store, (st) => st.source === "sim");
  const search = useRef<HTMLInputElement>(null);
  const total = ORDER.reduce((sum, k) => sum + counts[k], 0);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "/" && !isTyping(e.target) && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        search.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const chip = (kind: FilterKind, count: number, color: number, label: string) => {
    const pressed = filter.kinds.has(kind);
    const showLabel = count > 0 || pressed;
    return (
      <button
        key={kind}
        type="button"
        className={s.count}
        aria-pressed={pressed}
        title={`${count} ${label}`}
        data-testid={`count-${kind}`}
        onClick={() => store.getState().toggleFilterKind(kind)}
      >
        <span className={s.dot} style={{ background: hex(color) }} />
        {showLabel ? `${count} ${label}` : count}
      </button>
    );
  };

  return (
    <header className={s.topbar}>
      <span className={s.brand}>KITI</span>
      {simulated && (
        <span className={s.simBadge} title="Agentes generados por el simulador, no procesos reales">
          SIMULADO
        </span>
      )}
      <span className={s.nowrap}>{total} agentes</span>
      <span className={s.counts}>
        {ORDER.map((k) => chip(k, counts[k], STATUS_COLOR[k], LABEL[k]))}
        {dormant > 0 && chip("dormant", dormant, PAL.dormant, "apagadas")}
      </span>
      <span className={s.spacer} />
      <MailChip store={store} />
      <QuotaBadge store={store} now={now ?? Date.now} />
      <div className={s.layoutGroup} role="radiogroup" aria-label="Modo de distribución">
        {(["dynamic", "horizontal", "vertical"] as const).map((mode) => (
          <button
            key={mode}
            type="button"
            className={`${s.layoutBtn} ${layoutMode === mode ? s.layoutActive : ""}`}
            title={
              mode === "dynamic"
                ? "Distribución dinámica (centrada)"
                : mode === "horizontal"
                  ? "Distribución horizontal"
                  : "Distribución vertical"
            }
            aria-label={`Distribución ${mode}`}
            onClick={() => store.getState().setLayoutMode(mode)}
          >
            {mode === "dynamic" ? "⊞" : mode === "horizontal" ? "⊟" : "☷"}
          </button>
        ))}
      </div>
      <input
        ref={search}
        type="search"
        className={s.search}
        placeholder="Buscar ( / )"
        aria-label="Buscar oficinas"
        value={filter.text}
        onChange={(e) => store.getState().setFilterText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            store.getState().clearFilter();
            e.currentTarget.blur();
          }
        }}
      />
      {isActive(filter) && (
        <button type="button" className={s.button} onClick={() => store.getState().clearFilter()}>
          Quitar filtro
        </button>
      )}
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
