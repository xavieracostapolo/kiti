// Filtro de oficinas (Hito 13): por estado (los contadores de la barra) y por texto. Puro.
// Las oficinas que no coinciden se atenúan; el mapa no se reordena.

import type { AgentState, ConversationInfo, StatusKind } from "../protocol";

/** Estados del semáforo más las oficinas apagadas (conversaciones cerradas). */
export type FilterKind = StatusKind | "dormant";

export type OfficeFilter = { kinds: ReadonlySet<FilterKind>; text: string };

export const EMPTY_FILTER: OfficeFilter = { kinds: new Set(), text: "" };

export const isActive = (f: OfficeFilter) => f.kinds.size > 0 || f.text.trim() !== "";

export function toggleKind(f: OfficeFilter, kind: FilterKind): OfficeFilter {
  const kinds = new Set(f.kinds);
  if (kinds.has(kind)) kinds.delete(kind);
  else kinds.add(kind);
  return { ...f, kinds };
}

/** Sin mayúsculas ni tildes: "Migración" coincide con "migracion". */
const fold = (s: string) =>
  s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();

function textMatches(text: string, fields: (string | undefined)[]): boolean {
  const needle = fold(text.trim());
  if (needle === "") return true;
  return fields.some((f) => f !== undefined && fold(f).includes(needle));
}

export function matchesAgent(a: AgentState, f: OfficeFilter): boolean {
  if (f.kinds.size > 0 && !f.kinds.has(a.status.kind)) return false;
  return textMatches(f.text, [a.title, a.project, a.cwd, a.provider]);
}

export function matchesConversation(c: ConversationInfo, f: OfficeFilter): boolean {
  if (f.kinds.size > 0 && !f.kinds.has("dormant")) return false;
  return textMatches(f.text, [c.title, c.project, c.cwd, c.provider]);
}
