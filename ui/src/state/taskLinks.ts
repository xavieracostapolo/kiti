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
