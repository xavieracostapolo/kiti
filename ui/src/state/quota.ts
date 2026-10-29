// Resumen de la cuota de un proveedor para la barra superior (ADR 0015). Puro.

import type { QuotaWindow } from "../protocol";

export type QuotaLevel = "ok" | "warn" | "low";

export type QuotaSummary = {
  /** Lo que queda en la peor ventana corta (5 h), si la hay. */
  short?: number;
  /** Lo que queda en la peor ventana semanal, si la hay. */
  weekly?: number;
  level: QuotaLevel;
};

/** Verde por encima del 50 %, amarillo hasta el 20 % y rojo desde ahí (el aviso del core). */
export function levelOf(pct: number): QuotaLevel {
  if (pct <= 20) return "low";
  return pct <= 50 ? "warn" : "ok";
}

const min = (xs: number[]) => (xs.length ? Math.min(...xs) : undefined);

export function summarize(windows: readonly QuotaWindow[]): QuotaSummary {
  const short = min(windows.filter((w) => !w.weekly).map((w) => w.remainingPct));
  const weekly = min(windows.filter((w) => w.weekly).map((w) => w.remainingPct));
  const worst = min([short, weekly].filter((x): x is number => x !== undefined)) ?? 100;
  const summary: QuotaSummary = { level: levelOf(worst) };
  if (short !== undefined) summary.short = short;
  if (weekly !== undefined) summary.weekly = weekly;
  return summary;
}
