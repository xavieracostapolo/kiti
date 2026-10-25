// Oficinas del mundo (ADR 0009): las encendidas (agentes vivos) y las apagadas (conversaciones
// cerradas que se pueden retomar). Puro: sin Pixi ni React.

import type { RoomEntry } from "../domain/zones";
import type { AgentId, AgentState, ConversationInfo } from "../protocol";

const DORMANT = "conv:";

export const dormantId = (sessionId: string) => `${DORMANT}${sessionId}`;
export const isDormantId = (id: string) => id.startsWith(DORMANT);

/**
 * Conversaciones cerradas por id de oficina, sin las que ya tienen un agente encendido. Devuelve
 * `prev` si nada cambió, para que los suscriptores comparen por referencia.
 */
export function dormantRooms(
  agents: ReadonlyMap<AgentId, AgentState>,
  conversations: readonly ConversationInfo[],
  prev: ReadonlyMap<string, ConversationInfo>,
): ReadonlyMap<string, ConversationInfo> {
  const open = new Set<string>();
  for (const a of agents.values()) if (a.sessionId !== undefined) open.add(a.sessionId);
  const next = new Map<string, ConversationInfo>();
  for (const c of conversations) if (!open.has(c.sessionId)) next.set(dormantId(c.sessionId), c);
  if (next.size === prev.size && [...next].every(([id, c]) => prev.get(id) === c)) return prev;
  return next;
}

/** Entradas para el layout: primero las encendidas, después las apagadas (más recientes antes). */
export function roomEntries(
  agents: ReadonlyMap<AgentId, AgentState>,
  dormant: ReadonlyMap<string, ConversationInfo>,
): RoomEntry[] {
  const entries: RoomEntry[] = [];
  for (const a of agents.values()) entries.push({ id: a.id, project: a.project || a.cwd });
  for (const [id, c] of dormant) entries.push({ id, project: c.project || c.cwd });
  return entries;
}
