/** Plazo mínimo entre dos peticiones de la lista de terminales. */
export const TERMINAL_REFRESH_GAP_MS = 5_000;

export type TerminalRefreshInput = {
  /** pid de cada agente que hay ahora en el store. */
  agentPids: readonly number[];
  /** pid de las terminales que la UI ya conoce. */
  knownPids: ReadonlySet<number>;
  /** pid por los que ya se pidió la lista (con o sin éxito). */
  asked: ReadonlySet<number>;
  lastAt: number;
  now: number;
};

export type TerminalRefreshDecision = {
  refresh: boolean;
  /** pid a anotar como pedidos si se refresca. */
  mark: number[];
  /** Si hay pids pendientes pero falta plazo: ms hasta poder pedir. */
  waitMs?: number;
};

/**
 * Cuándo volver a pedir la lista de terminales: aparece un agente cuyo pid no tiene terminal
 * (sesión tmux creada fuera de Kiti). Cada pid se pide una sola vez, porque un agente externo
 * (Ghostty) nunca la tendrá, y como mucho una petición cada 5 s. Sin sondeo continuo.
 */
export function decideTerminalRefresh(i: TerminalRefreshInput): TerminalRefreshDecision {
  const mark = i.agentPids.filter((pid) => !i.knownPids.has(pid) && !i.asked.has(pid));
  if (mark.length === 0) return { refresh: false, mark: [] };
  const wait = i.lastAt + TERMINAL_REFRESH_GAP_MS - i.now;
  if (wait > 0) return { refresh: false, mark: [], waitMs: wait };
  return { refresh: true, mark };
}
