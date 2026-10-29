// Zonas por proyecto (ADR 0009): las oficinas de un mismo repositorio git (o carpeta) se agrupan
// en un área con su nombre. Puro: sin Pixi ni React.

import { assignSlots, type LayoutOpts, type Rect } from "./layout";

/** Una oficina a colocar: encendida (agente) o apagada (conversación cerrada). */
export type RoomEntry = { id: string; project: string };

export type Zone = Rect & {
  project: string;
  /** Nombre visible: última carpeta del proyecto. */
  name: string;
  count: number;
};

export type ZoneLayout = {
  zones: Zone[];
  rooms: Map<string, Rect>;
  bounds: Rect;
  /** Slots dentro de cada zona: pasarlo al siguiente cálculo para que nadie se mueva. */
  slots: ZoneSlots;
};

export type ZoneSlots = ReadonlyMap<string, ReadonlyMap<string, number>>;

/** Modo de distribución del mapa de zonas. */
export type LayoutMode = "dynamic" | "horizontal" | "vertical";

export type ZoneOpts = {
  /** Oficinas por fila dentro de una zona. */
  cols: number;
  /** Alto del rótulo de la zona, en tiles. */
  header: number;
  /** Margen interior de la zona, en tiles. */
  pad: number;
  /** Separación entre zonas, en tiles. */
  gap: number;
  /** Ancho máximo de una fila de zonas, en oficinas. */
  maxRowRooms: number;
  /** Modo de distribución (`dynamic` centra las filas y equilibra el aspecto). */
  mode?: LayoutMode;
  /** Relación ancho/alto objetivo para el modo `dynamic` (p. ej. `vp.w / vp.h`). */
  aspect?: number;
};

export const DEFAULT_ZONES: ZoneOpts = {
  cols: 3,
  header: 2,
  pad: 1,
  gap: 3,
  maxRowRooms: 8,
  mode: "dynamic",
};

export function projectName(project: string): string {
  const parts = project.split(/[/\\]/).filter((p) => p !== "");
  return parts.at(-1) ?? project;
}

/**
 * Elige cuántas oficinas de ancho máximo usar según el modo:
 * - `horizontal`: apaisado (mínimo 8 oficinas de ancho, o proporcional a la raíz del total).
 * - `vertical`: en columna (una zona por fila o hasta 3 oficinas de ancho).
 * - `dynamic`: equilibra ancho y alto según el número total de salas y la proporción de pantalla.
 */
export function targetRowRooms(
  totalRooms: number,
  mode: LayoutMode,
  fallbackMax: number,
  aspect = 1.6,
): number {
  if (mode === "vertical") return Math.min(3, Math.max(1, fallbackMax));
  if (mode === "horizontal") return Math.max(fallbackMax, Math.ceil(Math.sqrt(totalRooms) * 2.2));
  if (totalRooms <= 0) return fallbackMax;
  // En modo dinámico, buscamos que el bloque de salas se acerque al aspect ratio de la ventana.
  const ideal = Math.round(Math.sqrt(totalRooms * Math.max(0.6, Math.min(2.5, aspect))) * 1.35);
  return Math.max(4, Math.min(Math.max(fallbackMax, 12), ideal));
}

/**
 * Agrupa por proyecto y coloca las zonas en filas centradas (de izquierda a derecha, por nombre).
 * Dentro de una zona cada oficina conserva su slot mientras exista: solo se desplazan zonas enteras.
 */
export function layoutZones(
  prev: ZoneSlots,
  entries: readonly RoomEntry[],
  room: LayoutOpts,
  z: ZoneOpts = DEFAULT_ZONES,
): ZoneLayout {
  const groups = new Map<string, string[]>();
  for (const e of entries) {
    const ids = groups.get(e.project);
    if (ids) ids.push(e.id);
    else groups.set(e.project, [e.id]);
  }
  const projects = [...groups.keys()].sort((a, b) => {
    const byName = projectName(a).localeCompare(projectName(b));
    return byName !== 0 ? byName : a.localeCompare(b);
  });

  const t = room.tile;
  const stepX = (room.roomW + room.gap) * t;
  const stepY = (room.roomH + room.gap) * t;
  const mode = z.mode ?? "dynamic";
  const zoneCols = mode === "vertical" ? Math.min(z.cols, 2) : z.cols;
  const rowRooms =
    z.mode !== undefined
      ? targetRowRooms(entries.length, mode, z.maxRowRooms, z.aspect)
      : z.maxRowRooms;
  const maxRowW = rowRooms * stepX;

  type PlacedZone = {
    project: string;
    name: string;
    count: number;
    own: Map<string, number>;
    w: number;
    h: number;
    x: number;
    y: number;
  };

  const rowsList: PlacedZone[][] = [];
  let currentRow: PlacedZone[] = [];
  const slots = new Map<string, Map<string, number>>();
  let x = 0;
  let y = 0;
  let rowH = 0;
  for (const project of projects) {
    const ids = groups.get(project) as string[];
    const own = assignSlots(prev.get(project) ?? new Map(), ids);
    slots.set(project, own);
    const used = Math.max(...own.values()) + 1;
    const cols = Math.min(zoneCols, used);
    const rCount = Math.ceil(used / zoneCols);
    const w = cols * stepX - room.gap * t + 2 * z.pad * t;
    const h = (z.header + z.pad) * t + rCount * stepY - room.gap * t + z.pad * t;
    if (x > 0 && (mode === "vertical" || x + w > maxRowW)) {
      rowsList.push(currentRow);
      currentRow = [];
      x = 0;
      y += rowH + z.gap * t;
      rowH = 0;
    }
    currentRow.push({ x, y, w, h, project, name: projectName(project), count: ids.length, own });
    x += w + z.gap * t;
    rowH = Math.max(rowH, h);
  }
  if (currentRow.length > 0) rowsList.push(currentRow);

  let maxW = 0;
  for (const row of rowsList) {
    const last = row.at(-1);
    if (last) maxW = Math.max(maxW, last.x + last.w);
  }

  // En modo dynamic o horizontal con centrado simétrico: cada fila se centra respecto al ancho total
  // cuando z.mode está activo, evitando que las filas queden pegadas a la izquierda.
  const centerRows = z.mode === "dynamic" || z.mode === "horizontal";
  const zones: Zone[] = [];
  const rooms = new Map<string, Rect>();
  for (const row of rowsList) {
    const last = row.at(-1);
    const rowW = last ? last.x + last.w : 0;
    const offsetX = centerRows ? Math.round((maxW - rowW) / 2) : 0;
    for (const pz of row) {
      const zx = pz.x + offsetX;
      const zy = pz.y;
      zones.push({
        x: zx,
        y: zy,
        w: pz.w,
        h: pz.h,
        project: pz.project,
        name: pz.name,
        count: pz.count,
      });
      for (const [id, slot] of pz.own) {
        rooms.set(id, {
          x: zx + z.pad * t + (slot % zoneCols) * stepX,
          y: zy + (z.header + z.pad) * t + Math.floor(slot / zoneCols) * stepY,
          w: room.roomW * t,
          h: room.roomH * t,
        });
      }
    }
  }

  let right = room.roomW * t;
  let bottom = room.roomH * t;
  for (const zone of zones) {
    right = Math.max(right, zone.x + zone.w);
    bottom = Math.max(bottom, zone.y + zone.h);
  }
  return { zones, rooms, bounds: { x: 0, y: 0, w: right, h: bottom }, slots };
}
