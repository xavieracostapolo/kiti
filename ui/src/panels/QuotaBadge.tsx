import { HoverCard } from "radix-ui";
import { useStore } from "zustand";
import type { Provider, QuotaWindow } from "../protocol";
import { type QuotaLevel, summarize } from "../state/quota";
import type { KitiStore } from "../state/store";
import { inTime } from "./labels";
import s from "./panels.module.css";

const NAME: Record<Provider, string> = {
  claude: "Claude",
  codex: "Codex",
  gemini: "Gemini",
  antigravity: "Antigravity",
};

const COLOR: Record<QuotaLevel, string> = {
  ok: "#3ddc84",
  warn: "#ffc93c",
  low: "#ff5c5c",
};

type Props = { store: KitiStore; now: () => number };

/**
 * Cuánta cuota queda de cada proveedor (ADR 0015): lo semanal a la vista, con el color de lo
 * peor entre 5 h y semana; el detalle de cada ventana y cuándo se reinicia, al pasar el ratón. Va aparte de
 * la barra para poder moverlo a otro sitio sin tocar nada más.
 */
export function QuotaBadge({ store, now }: Props) {
  const quotas = useStore(store, (st) => st.quotas);
  if (quotas.size === 0) return null;
  return (
    <span className={s.quotas}>
      {[...quotas].map(([provider, windows]) => (
        <QuotaChip key={provider} provider={provider} windows={windows} now={now} />
      ))}
    </span>
  );
}

function QuotaChip({
  provider,
  windows,
  now,
}: {
  provider: Provider;
  windows: readonly QuotaWindow[];
  now: () => number;
}) {
  const sum = summarize(windows);
  const parts = [
    sum.short !== undefined ? `5 h ${sum.short} %` : undefined,
    sum.weekly !== undefined ? `sem ${sum.weekly} %` : undefined,
  ].filter(Boolean);
  const label = `Cuota de ${NAME[provider]}: ${parts.join(", ")}`;
  return (
    <HoverCard.Root openDelay={150} closeDelay={100}>
      <HoverCard.Trigger asChild>
        <button type="button" className={s.quotaChip} aria-label={label}>
          <span className={s.dot} style={{ background: COLOR[sum.level] }} />
          {/* Compacto: lo semanal (o la de 5 h si no hay semanal); el color, lo peor de las dos. */}
          {NAME[provider]} {sum.weekly ?? sum.short} %
        </button>
      </HoverCard.Trigger>
      <HoverCard.Portal>
        <HoverCard.Content className={s.menu} sideOffset={6} align="end">
          <ul className={s.quotaList} aria-label={`Detalle de la cuota de ${NAME[provider]}`}>
            {windows.map((w) => (
              <li key={w.label}>
                <strong>{w.label}</strong>: queda {w.remainingPct} %
                {w.resetsAt > 0 && ` · se reinicia ${inTime(w.resetsAt - now())}`}
              </li>
            ))}
          </ul>
        </HoverCard.Content>
      </HoverCard.Portal>
    </HoverCard.Root>
  );
}
