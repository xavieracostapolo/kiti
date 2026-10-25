// Tamaños de las oficinas y slots estables (ADR 0003 §5). Las zonas por proyecto están en
// zones.ts (ADR 0009). Puro: sin Pixi ni React.

export type Rect = { x: number; y: number; w: number; h: number };

export type LayoutOpts = {
  /** Tamaño de una oficina en tiles. */
  roomW: number;
  roomH: number;
  /** Pasillo entre oficinas, en tiles. */
  gap: number;
  /** Lado de un tile en px de mundo. */
  tile: number;
};

/** Ancho del panel de terminal como fracción del viewport (debe coincidir con `.terminalPanel`). */
export const TERMINAL_FRACTION = 0.46;

export const DEFAULT_LAYOUT: LayoutOpts = { roomW: 12, roomH: 9, gap: 2, tile: 16 };

/**
 * Los supervivientes conservan su slot; cada id nuevo ocupa el hueco libre más bajo,
 * en el orden en que llega. Nadie se mueve cuando otro agente desaparece.
 */
export function assignSlots(
  prev: ReadonlyMap<string, number>,
  ids: readonly string[],
): Map<string, number> {
  const next = new Map<string, number>();
  const used = new Set<number>();
  for (const id of ids) {
    const slot = prev.get(id);
    if (slot !== undefined) {
      next.set(id, slot);
      used.add(slot);
    }
  }
  let free = 0;
  for (const id of ids) {
    if (next.has(id)) continue;
    while (used.has(free)) free++;
    next.set(id, free);
    used.add(free);
  }
  return next;
}
