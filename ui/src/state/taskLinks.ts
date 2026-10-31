// Líneas del mapa entre un orquestador y los agentes efímeros que lanzó (ADR 0019).
import type { AgentId, MailState, TaskInfo } from "../protocol";

export type TaskLink = {
  /** Oficina del orquestador. */
  from: AgentId;
  /** Oficina del subagente. */
  to: AgentId;
  /** La tarea está atascada: la línea sale en ámbar. */
  stuck: boolean;
  fromAlias: string;
  role: string;
};

/** Lo que muestra la etiqueta de una oficina: quién la lanzó y cuántos subagentes vivos tiene. */
export type OfficeTaskBadge = { launched: number; by?: string; role?: string };

/** Una línea por tarea viva cuyo orquestador y subagente tienen oficina (alias → id por `mail`). */
export function taskLinks(tasks: readonly TaskInfo[], mail: MailState | undefined): TaskLink[] {
  if (!mail) return [];
  const byAlias = new Map(mail.aliases.map((a) => [a.alias, a.agent]));
  const out: TaskLink[] = [];
  for (const t of tasks) {
    if (t.state !== "launching" && t.state !== "running" && t.state !== "stuck") continue;
    const from = byAlias.get(t.orchestrator);
    const to = t.alias == null ? undefined : byAlias.get(t.alias);
    if (from === undefined || to === undefined) continue;
    out.push({ from, to, stuck: t.state === "stuck", fromAlias: t.orchestrator, role: t.role });
  }
  return out;
}

export function officeTaskBadges(links: readonly TaskLink[]): Map<AgentId, OfficeTaskBadge> {
  const out = new Map<AgentId, OfficeTaskBadge>();
  const get = (id: AgentId) => {
    const b = out.get(id) ?? { launched: 0 };
    out.set(id, b);
    return b;
  };
  for (const l of links) {
    get(l.from).launched++;
    const sub = get(l.to);
    sub.by = l.fromAlias;
    sub.role = l.role;
  }
  return out;
}

type Box = { x: number; y: number; w: number; h: number };

/** Segmento entre los bordes de dos oficinas y ángulo (rad) de la flecha, que apunta al subagente. */
export type ClippedLink = { x1: number; y1: number; x2: number; y2: number; angle: number };

/**
 * Recorta el segmento centro a centro contra los dos rectángulos: sale del borde del orquestador
 * y llega al del subagente. Sin tramo visible (se tocan o se solapan) devuelve `undefined`.
 */
export function clipLink(from: Box, to: Box): ClippedLink | undefined {
  const ax = from.x + from.w / 2;
  const ay = from.y + from.h / 2;
  const dx = to.x + to.w / 2 - ax;
  const dy = to.y + to.h / 2 - ay;
  if (dx === 0 && dy === 0) return undefined;
  // Parámetro (en longitudes del segmento) en que sale de un rectángulo desde su centro, con dirección (ux, uy).
  const exit = (r: Box, ux: number, uy: number) => {
    const tx = ux === 0 ? Infinity : r.w / 2 / Math.abs(ux);
    const ty = uy === 0 ? Infinity : r.h / 2 / Math.abs(uy);
    return Math.min(tx, ty);
  };
  const tOut = exit(from, dx, dy);
  const tIn = 1 - exit(to, dx, dy);
  if (!(tOut < tIn)) return undefined;
  return {
    x1: ax + dx * tOut,
    y1: ay + dy * tOut,
    x2: ax + dx * tIn,
    y2: ay + dy * tIn,
    angle: Math.atan2(dy, dx),
  };
}
