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

/** Rectángulo mínimo que contiene a los dos. */
export function union(a: Rect, b: Rect): Rect {
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return { x, y, w: Math.max(a.x + a.w, b.x + b.w) - x, h: Math.max(a.y + a.h, b.y + b.h) - y };
}

/** Ancho del panel de terminal como fracción del viewport (por defecto y límites al redimensionar). */
export const TERMINAL_FRACTION = 0.46;
export const MIN_TERMINAL_FRACTION = 0.32;
export const MAX_TERMINAL_FRACTION = 0.72;

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
