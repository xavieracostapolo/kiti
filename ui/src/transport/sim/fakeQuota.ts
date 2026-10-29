// Cuota de uso falsa para el modo sim (`?quota=1`, ADR 0015): Claude con margen y Antigravity
// con un límite semanal casi agotado, para ver los tres colores.
import type { ServerMsg } from "../../protocol";

const HOUR = 3_600_000;

export function fakeQuota(now: number): ServerMsg[] {
  return [
    {
      type: "Quota",
      provider: "claude",
      windows: [
        { label: "5 h", weekly: false, remainingPct: 72, resetsAt: now + 3 * HOUR },
        { label: "semana", weekly: true, remainingPct: 40, resetsAt: now + 52 * HOUR },
      ],
    },
    {
      type: "Quota",
      provider: "antigravity",
      windows: [
        {
          label: "Gemini Models · semana",
          weekly: true,
          remainingPct: 100,
          resetsAt: now + 150 * HOUR,
        },
        {
          label: "Gemini Models · 5 h",
          weekly: false,
          remainingPct: 100,
          resetsAt: now + 4 * HOUR,
        },
        {
          label: "Claude and GPT models · semana",
          weekly: true,
          remainingPct: 18,
          resetsAt: now + 98 * HOUR,
        },
        {
          label: "Claude and GPT models · 5 h",
          weekly: false,
          remainingPct: 90,
          resetsAt: now + 4 * HOUR,
        },
      ],
    },
  ];
}
