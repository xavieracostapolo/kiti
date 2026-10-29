// Mensajes entre agentes (ADR 0017): lo que la UI deriva del estado que manda el core. Puro.

import type { AgentId, MailMessage, MailState } from "../protocol";

const WAITING: readonly MailMessage["state"][] = ["pending", "queued", "held"];

const isWaiting = (m: MailMessage) => WAITING.includes(m.state);

function* allMessages(mail: MailState) {
  for (const t of mail.threads) yield* t.messages;
}

/** Alias con el que los demás agentes le escriben (`claude-1`). */
export function aliasOf(mail: MailState | undefined, id: AgentId): string | undefined {
  return mail?.aliases.find((a) => a.agent === id)?.alias;
}

/** Mensajes aún sin entregar a `id`: el sobre de su oficina. */
export function waitingFor(mail: MailState | undefined, id: AgentId): number {
  if (!mail) return 0;
  let n = 0;
  for (const m of allMessages(mail)) if (m.to === id && isWaiting(m)) n++;
  return n;
}

export type Transit = { from: AgentId; to: AgentId; count: number };

/** Parejas con mensajes en camino: un sobre entre sus dos oficinas. */
export function inTransit(mail: MailState | undefined): Transit[] {
  if (!mail) return [];
  const out = new Map<string, Transit>();
  for (const m of allMessages(mail)) {
    if (!isWaiting(m)) continue;
    const key = `${m.from}>${m.to}`;
    const t = out.get(key);
    if (t) t.count++;
    else out.set(key, { from: m.from, to: m.to, count: 1 });
  }
  return [...out.values()];
}

/**
 * Chip de la barra superior: con algo por decidir (mensaje por aprobar o hilo pausado), cuántas
 * cosas y en amarillo; si no, cuántos mensajes van en camino.
 */
export function chipOf(
  mail: MailState | undefined,
): { count: number; attention: boolean } | undefined {
  if (!mail) return undefined;
  if (mail.attention > 0) return { count: mail.attention, attention: true };
  let n = 0;
  for (const m of allMessages(mail)) if (isWaiting(m)) n++;
  return { count: n, attention: false };
}
