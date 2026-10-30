// Tareas falsas de agentes efímeros para el modo sim (`?tasks=1`, ADR 0019): una de cada estado.
// Provisional: el core las mandará en un `ServerMsg::Tasks`.
import type { AgentState, Provider } from "../../protocol";
import type { TaskInfo } from "../../state/tasks";

const PREFIX: Record<Provider, string> = {
  claude: "claude",
  codex: "codex",
  gemini: "gemini",
  antigravity: "agy",
};

const MIN = 60_000;

/** Alias de oficina como en `fakeMail`: `claude-1`, `codex-1`, `agy-1`... */
function aliasesOf(agents: readonly AgentState[]): string[] {
  const counters = new Map<string, number>();
  return agents.map((a) => {
    const prefix = PREFIX[a.provider];
    const n = (counters.get(prefix) ?? 0) + 1;
    counters.set(prefix, n);
    return `${prefix}-${n}`;
  });
}

export function fakeTasks(agents: readonly AgentState[], now: number): TaskInfo[] {
  const aliases = aliasesOf(agents);
  const orchestrator = aliases[0] ?? "claude-1";
  const base = { orchestrator };
  // Alias de la oficina `n` (desde 0) del proveedor, para que alias y proveedor concuerden.
  const withAlias = (provider: Provider, n: number): { alias?: string } => {
    const mine = aliases.filter((x) => x.startsWith(`${PREFIX[provider]}-`));
    const alias = mine[n % Math.max(1, mine.length)];
    return alias ? { alias } : {};
  };
  return [
    {
      ...base,
      ...withAlias("claude", 1),
      id: 1,
      role: "implementar",
      provider: "claude",
      model: "sonnet",
      branch: "feat/carrito-descuentos",
      session: "kiti-31",
      state: "running",
      createdAt: now - 12 * MIN,
    },
    {
      ...base,
      ...withAlias("codex", 0),
      id: 2,
      role: "documentar",
      provider: "codex",
      model: "gpt-5",
      branch: "docs/guia-envios",
      state: "launching",
      createdAt: now - 20_000,
    },
    {
      ...base,
      ...withAlias("claude", 2),
      id: 3,
      role: "revisar",
      provider: "claude",
      model: "opus",
      branch: "fix/ws-leak",
      session: "kiti-32",
      state: "stuck",
      reason: "sin actividad desde hace 10 min",
      createdAt: now - 45 * MIN,
    },
    {
      ...base,
      ...withAlias("antigravity", 0),
      id: 4,
      role: "implementar",
      provider: "antigravity",
      model: "gemini-3",
      branch: "feat/pedidos-migracion",
      state: "failed",
      reason: "el agente terminó con error",
      createdAt: now - 3 * 3_600_000,
      finishedAt: now - 2 * 3_600_000,
    },
    {
      ...base,
      ...withAlias("claude", 3),
      id: 5,
      role: "implementar",
      provider: "claude",
      model: "sonnet",
      branch: "feat/tienda-checkout",
      pr: "https://github.com/ejemplo/tienda-web/pull/12",
      state: "done",
      createdAt: now - 5 * 3_600_000,
      finishedAt: now - 4 * 3_600_000,
    },
  ];
}
