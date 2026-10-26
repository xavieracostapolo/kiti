// Tablero de la oficina en el zoom cercano (ADR 0016): qué texto lleva y cómo se reparte la
// última hora en la línea de tiempo. Puro.

import type { AgentState, GitInfo, StatusKind } from "../protocol";

const HOUR = 3_600_000;

/** Líneas del tablero; las que no tienen datos no aparecen. */
export function boardLines(agent: AgentState, git: GitInfo | null | undefined): string[] {
  const lines: string[] = [];
  if (agent.model) lines.push(agent.model);
  if (agent.contextPct !== undefined) lines.push(`contexto ${agent.contextPct} %`);
  if (agent.costCents !== undefined) lines.push(`$${(agent.costCents / 100).toFixed(2)} sesión`);
  if (git) {
    const parts = [git.branch ?? "HEAD separado"];
    if (git.changed > 0) parts.push(`${git.changed} cambios`);
    if (git.ahead > 0) parts.push(`↑${git.ahead}`);
    if (git.behind > 0) parts.push(`↓${git.behind}`);
    lines.push(parts.join(" · "));
  }
  return lines;
}

export type Segment = { kind: StatusKind; from: number; to: number };

/**
 * Tramos de la última `span` (ms) hasta `now`, como fracciones de 0 a 1. `history` va del más
 * viejo al más nuevo; lo anterior a la ventana se recorta y el último tramo llega hasta `now`.
 */
export function timeline(
  history: readonly { kind: StatusKind; since: number }[],
  now: number,
  span = HOUR,
): Segment[] {
  const start = now - span;
  const out: Segment[] = [];
  history.forEach((h, i) => {
    const end = history[i + 1]?.since ?? now;
    const from = Math.max(h.since, start);
    const to = Math.min(end, now);
    if (to <= from) return;
    out.push({ kind: h.kind, from: (from - start) / span, to: (to - start) / span });
  });
  return out;
}
