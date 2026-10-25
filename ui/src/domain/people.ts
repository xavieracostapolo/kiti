// Qué persona se sienta en cada oficina (ADR 0013): elegida por el id del agente, así es siempre
// la misma y se distinguen las oficinas de un vistazo. Puro.

/** FNV-1a de 32 bits: rápido, estable y bien repartido para ids cortos. */
function fnv1a(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** Índice de persona (de 0 a `count` - 1) para el agente `id`. */
export function personFor(id: string, count: number): number {
  return fnv1a(id) % count;
}
