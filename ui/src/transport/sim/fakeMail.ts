// Mensajes entre agentes falsos para el modo sim (`?mail=1`, ADR 0017): un hilo de cada caso
// (revisión, consulta, reparto) y otro pausado por el límite, con las acciones del panel.
import type {
  AgentState,
  MailCommand,
  MailMessage,
  MailState,
  MailThread,
  Provider,
} from "../../protocol";

const PREFIX: Record<Provider, string> = {
  claude: "claude",
  codex: "codex",
  gemini: "gemini",
  antigravity: "agy",
};

const MIN = 60_000;
const HOP_STEP = 12;

const waiting = (m: MailMessage) =>
  m.state === "pending" || m.state === "queued" || m.state === "held";

function attention(threads: readonly MailThread[]): number {
  let n = 0;
  for (const t of threads) {
    if (t.state === "paused") n++;
    for (const m of t.messages) if (m.state === "pending") n++;
  }
  return n;
}

/** Estado inicial a partir de los agentes del simulador (determinista). */
export function fakeMail(agents: readonly AgentState[], now: number): MailState {
  const counters = new Map<string, number>();
  const aliases = agents.map((a) => {
    const prefix = PREFIX[a.provider];
    const n = (counters.get(prefix) ?? 0) + 1;
    counters.set(prefix, n);
    return { agent: a.id, alias: `${prefix}-${n}` };
  });
  const aliasOf = (a: AgentState) => aliases.find((x) => x.agent === a.id)?.alias ?? "?";
  const used = new Set<string>();
  const take = (pred: (a: AgentState) => boolean = () => true) => {
    const a = agents.find((x) => !used.has(x.id) && pred(x)) ?? agents.find((x) => !used.has(x.id));
    if (a) used.add(a.id);
    return a;
  };
  let nextId = 0;
  const m = (
    from: AgentState,
    to: AgentState,
    text: string,
    ago: number,
    state: MailMessage["state"],
  ): MailMessage => ({
    id: ++nextId,
    from: from.id,
    fromAlias: aliasOf(from),
    to: to.id,
    toAlias: aliasOf(to),
    text,
    sentAt: now - ago,
    state,
  });
  const threads: MailThread[] = [];
  const thread = (subject: string, state: MailThread["state"], messages: MailMessage[]) =>
    threads.push({
      id: threads.length + 1,
      subject,
      hops: state === "paused" ? HOP_STEP + 1 : messages.length,
      limit: HOP_STEP,
      state,
      messages,
    });

  // Revisión: el primer Claude pide a un vecino de su proyecto que revise su arreglo.
  const reviewer = agents.find((a) => a.provider === "claude");
  if (reviewer) used.add(reviewer.id);
  const peer = take((a) => a.project === reviewer?.project && a.provider !== "claude");
  if (reviewer && peer) {
    thread("Revisión", "open", [
      m(
        reviewer,
        peer,
        "Terminé el arreglo de la fuga de memoria en el cliente de websockets (rama fix/ws-leak). ¿Puedes revisar el diff y decirme si se me escapa algún listener sin cerrar?",
        4_000,
        "queued",
      ),
    ]);
  }
  // Consulta entre dos proyectos, ya respondida.
  const asker = take((a) => a.provider !== "claude");
  const expert = take((a) => a.provider === "claude" && a.project !== asker?.project);
  if (asker && expert) {
    thread("Consulta", "open", [
      m(
        asker,
        expert,
        "¿El endpoint /orders devuelve el total con impuestos o sin ellos?",
        3 * MIN,
        "delivered",
      ),
      m(
        expert,
        asker,
        "Sin impuestos. Están en el campo tax_cents, en orders.rs línea 88.",
        2 * MIN,
        "delivered",
      ),
    ]);
  }
  // Reparto: un jefe da tareas a dos agentes; uno ya la tiene y el otro está ocupado.
  const lead = take((a) => a.provider === "claude");
  const w1 = take();
  const w2 = take();
  if (lead && w1 && w2) {
    thread("Reparto", "open", [
      m(
        lead,
        w1,
        "Te toca la parte de tests del carrito: cubre los descuentos por cantidad.",
        6 * MIN,
        "queued",
      ),
      m(
        lead,
        w2,
        "Tú revisa la migración de pedidos y avísame si rompe algo.",
        6 * MIN,
        "delivered",
      ),
    ]);
  }
  // Un debate que llegó al límite de saltos y espera al usuario.
  const d1 = take();
  const d2 = take();
  if (d1 && d2) {
    thread("Debate", "paused", [
      m(d1, d2, "Sigo pensando que la caché debería ir en el cliente.", 9 * MIN, "delivered"),
      m(d2, d1, "Y yo que en el servidor: invalidarla en el cliente es un lío.", 8 * MIN, "held"),
    ]);
  }
  threads.reverse(); // del más reciente al más antiguo, como el core
  threads.sort((a, b) => (b.messages.at(-1)?.sentAt ?? 0) - (a.messages.at(-1)?.sentAt ?? 0));
  return { mode: "auto", paused: false, attention: attention(threads), aliases, threads };
}

const withState = (t: MailThread, f: (m: MailMessage) => MailMessage): MailThread => ({
  ...t,
  messages: t.messages.map(f),
});

/** Lo que haría el core con cada acción del panel. */
export function applyMailCommand(mail: MailState, cmd: MailCommand): MailState {
  let threads = mail.threads;
  let { mode, paused } = mail;
  switch (cmd.kind) {
    case "mode":
      mode = cmd.mode;
      if (mode === "auto") {
        threads = threads.map((t) =>
          withState(t, (m) => (m.state === "pending" ? { ...m, state: "queued" } : m)),
        );
      }
      break;
    case "pause":
      paused = cmd.paused;
      // Como el core: al reanudar se entrega lo que estaba en cola.
      if (!paused && mail.paused) {
        threads = threads.map((t) =>
          withState(t, (m) => (m.state === "queued" ? { ...m, state: "delivered" } : m)),
        );
      }
      break;
    case "approve":
    case "discard": {
      const next = cmd.kind === "approve" ? "queued" : "discarded";
      threads = threads.map((t) =>
        withState(t, (m) =>
          m.id === cmd.id && (m.state === "pending" || (next === "discarded" && waiting(m)))
            ? { ...m, state: next }
            : m,
        ),
      );
      break;
    }
    case "continue":
      threads = threads.map((t) =>
        t.id === cmd.id && t.state === "paused"
          ? withState({ ...t, state: "open", limit: Math.max(t.hops, t.limit) + HOP_STEP }, (m) =>
              m.state === "held" ? { ...m, state: mode === "auto" ? "queued" : "pending" } : m,
            )
          : t,
      );
      break;
    case "close":
      threads = threads.map((t) =>
        t.id === cmd.id
          ? withState({ ...t, state: "closed" }, (m) =>
              waiting(m) ? { ...m, state: "discarded" } : m,
            )
          : t,
      );
      break;
  }
  return { ...mail, mode, paused, threads, attention: attention(threads) };
}
