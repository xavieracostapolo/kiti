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
};

export const DEFAULT_ZONES: ZoneOpts = { cols: 3, header: 2, pad: 1, gap: 3, maxRowRooms: 8 };

export function projectName(project: string): string {
  const parts = project.split(/[/\\]/).filter((p) => p !== "");
  return parts.at(-1) ?? project;
}

/**
 * Agrupa por proyecto y coloca las zonas en filas (de izquierda a derecha, por nombre). Dentro de
 * una zona cada oficina conserva su slot mientras exista: solo se desplazan zonas enteras.
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
  const maxRowW = z.maxRowRooms * stepX;

  const zones: Zone[] = [];
  const rooms = new Map<string, Rect>();
  const slots = new Map<string, Map<string, number>>();
  let x = 0;
  let y = 0;
  let rowH = 0;
  for (const project of projects) {
    const ids = groups.get(project) as string[];
    const own = assignSlots(prev.get(project) ?? new Map(), ids);
    slots.set(project, own);
    const used = Math.max(...own.values()) + 1;
    const cols = Math.min(z.cols, used);
    const rows = Math.ceil(used / z.cols);
    const w = cols * stepX - room.gap * t + 2 * z.pad * t;
    const h = (z.header + z.pad) * t + rows * stepY - room.gap * t + z.pad * t;
    if (x > 0 && x + w > maxRowW) {
      x = 0;
      y += rowH + z.gap * t;
      rowH = 0;
    }
    zones.push({ x, y, w, h, project, name: projectName(project), count: ids.length });
    for (const [id, slot] of own) {
      rooms.set(id, {
        x: x + z.pad * t + (slot % z.cols) * stepX,
        y: y + (z.header + z.pad) * t + Math.floor(slot / z.cols) * stepY,
        w: room.roomW * t,
        h: room.roomH * t,
      });
    }
    x += w + z.gap * t;
    rowH = Math.max(rowH, h);
  }

  let right = room.roomW * t;
  let bottom = room.roomH * t;
  for (const zone of zones) {
    right = Math.max(right, zone.x + zone.w);
    bottom = Math.max(bottom, zone.y + zone.h);
  }
  return { zones, rooms, bounds: { x: 0, y: 0, w: right, h: bottom }, slots };
}
