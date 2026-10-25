// Layout de oficinas por slots estables (ADR 0003 §5). Puro: sin Pixi ni React.

export type Rect = { x: number; y: number; w: number; h: number };

export type LayoutOpts = {
  /** Oficinas por fila. */
  cols: number;
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

export const DEFAULT_LAYOUT: LayoutOpts = { cols: 5, roomW: 12, roomH: 9, gap: 2, tile: 16 };

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

export function slotRect(slot: number, opts: LayoutOpts): Rect {
  const col = slot % opts.cols;
  const row = Math.floor(slot / opts.cols);
  const stepX = (opts.roomW + opts.gap) * opts.tile;
  const stepY = (opts.roomH + opts.gap) * opts.tile;
  return { x: col * stepX, y: row * stepY, w: opts.roomW * opts.tile, h: opts.roomH * opts.tile };
}

/** Rectángulo que contiene todas las oficinas ocupadas (al menos una sala). */
export function worldBounds(slots: ReadonlyMap<string, number>, opts: LayoutOpts): Rect {
  let right = opts.roomW * opts.tile;
  let bottom = opts.roomH * opts.tile;
  for (const slot of slots.values()) {
    const r = slotRect(slot, opts);
    right = Math.max(right, r.x + r.w);
    bottom = Math.max(bottom, r.y + r.h);
  }
  return { x: 0, y: 0, w: right, h: bottom };
}
