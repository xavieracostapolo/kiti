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

export type Transit = { from: AgentId; to: AgentId; count: number; progress: number };

/** Lo entregado sigue en el mapa este rato, para que una entrega instantánea se vea. */
export const MAIL_LINGER_MS = 3000;

/** Parejas con mensajes en camino: un sobre entre sus dos oficinas. */
export function inTransit(mail: MailState | undefined): Transit[] {
  if (!mail) return [];
  const out = new Map<string, Transit>();
  for (const m of allMessages(mail)) {
    if (!isWaiting(m)) continue;
    const key = `${m.from}>${m.to}`;
    const t = out.get(key);
    if (t) t.count++;
    else out.set(key, { from: m.from, to: m.to, count: 1, progress: 0.5 });
  }
  return [...out.values()];
}

/**
 * Anota la hora en que la UI vio pasar cada mensaje a entregado. Sin estado anterior (primer
 * estado del core) el historial no cuenta: solo se anima lo que ocurre con la UI abierta.
 */
export function markDelivered(
  mail: MailState | undefined,
  prev: MailState | undefined,
  seen: Map<number, number>,
  now: number,
): void {
  if (!mail || !prev) return;
  const before = new Set<number>();
  for (const m of allMessages(prev)) if (m.state === "delivered") before.add(m.id);
  const alive = new Set<number>();
  for (const m of allMessages(mail)) {
    alive.add(m.id);
    if (m.state === "delivered" && !before.has(m.id) && !seen.has(m.id)) seen.set(m.id, now);
  }
  for (const id of seen.keys()) if (!alive.has(id)) seen.delete(id);
}

/**
 * Sobres en el mapa: los que esperan (a medio camino) y los entregados hace menos de
 * `MAIL_LINGER_MS`, que van de la oficina del remitente a la del destinatario.
 */
export function visibleTransit(
  mail: MailState | undefined,
  now: number,
  seen: ReadonlyMap<number, number>,
): Transit[] {
  const out = inTransit(mail);
  if (!mail) return out;
  const delivered = new Map<string, Transit>();
  for (const m of allMessages(mail)) {
    if (m.state !== "delivered") continue;
    const at = seen.get(m.id);
    if (at === undefined) continue;
    const progress = (now - at) / MAIL_LINGER_MS;
    if (progress < 0 || progress >= 1) continue;
    const key = `${m.from}>${m.to}`;
    const t = delivered.get(key);
    if (t) {
      t.count++;
      t.progress = Math.min(t.progress, progress);
    } else delivered.set(key, { from: m.from, to: m.to, count: 1, progress });
  }
  return [...out, ...delivered.values()];
}

/** ¿Queda un sobre entregado moviéndose? Solo entonces el mapa necesita frames. */
export function hasLingering(
  mail: MailState | undefined,
  now: number,
  seen: ReadonlyMap<number, number>,
): boolean {
  if (!mail) return false;
  for (const m of allMessages(mail)) {
    const at = seen.get(m.id);
    if (m.state === "delivered" && at !== undefined && now - at < MAIL_LINGER_MS) return true;
  }
  return false;
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
