// Flota de agentes simulados, determinista con su semilla y con reloj virtual (ADR 0003 §2).
// Sin Date.now() ni Math.random(): un generador xoroshiro128+ por agente.

import { uniformFloat64 } from "pure-rand/distribution/uniformFloat64";
import { uniformInt } from "pure-rand/distribution/uniformInt";
import { xoroshiro128plus } from "pure-rand/generator/xoroshiro128plus";
import type { RandomGenerator } from "pure-rand/types/RandomGenerator";
import profilesJson from "../../../../fixtures/sim/profiles.json";
import type { AgentId, AgentState, Provider } from "../../protocol";
import { type AgentStatus, applySignal, type Signal } from "./signals";

type Profile = (typeof profilesJson.profiles)[number];
type ProviderSpec = { provider: Provider; weight: number; tools: string[] };

const config = profilesJson as typeof profilesJson & {
  providers: ProviderSpec[];
  activities: Record<string, string>;
};

export const TICK_MS = config.tickMs;

type SimAgent = {
  state: AgentState;
  index: number;
  turns: number;
  rng: RandomGenerator;
  profile: Profile;
  tools: readonly string[];
  toolsLeft: number;
  toolOpen: boolean;
  permissionPending: boolean;
  nextAt: number;
};

export type FleetOptions = { count: number; seed: number };
export type FleetStep = { upserts: AgentState[]; removed: AgentId[] };

function pick<T extends { weight: number }>(rng: RandomGenerator, items: readonly T[]): T {
  let u = uniformFloat64(rng) * items.reduce((s, i) => s + i.weight, 0);
  for (const item of items) {
    u -= item.weight;
    if (u < 0) return item;
  }
  return items[items.length - 1] as T;
}

const chance = (rng: RandomGenerator, p: number) => uniformFloat64(rng) < p;
const expMs = (rng: RandomGenerator, mean: number) =>
  Math.max(TICK_MS, Math.round(-mean * Math.log(1 - uniformFloat64(rng))));

/** Oficinas por proyecto, en ciclo (ADR 0009). Igual que `project_index` del simulador del core. */
const GROUP_SIZES = [3, 1, 4, 2, 2, 1, 3, 2];
const CYCLE = GROUP_SIZES.reduce((a, b) => a + b, 0);

/** Proyecto del agente `index`: no depende de cuántos agentes haya. */
export function projectFor(index: number): string {
  let rest = index % CYCLE;
  let group = Math.floor(index / CYCLE) * GROUP_SIZES.length;
  for (const size of GROUP_SIZES) {
    if (rest < size) break;
    rest -= size;
    group++;
  }
  return config.projects[group % config.projects.length] as string;
}

export class SimFleet {
  private readonly agents = new Map<AgentId, SimAgent>();
  private readonly pendingKills = new Set<AgentId>();
  now: number = config.epochMs;

  constructor({ count, seed }: FleetOptions) {
    for (let i = 0; i < count; i++) {
      const agent = this.create(i, seed);
      this.agents.set(agent.state.id, agent);
    }
  }

  private create(index: number, seed: number): SimAgent {
    // seed ^ índice, mezclado para que semillas vecinas no den secuencias parecidas.
    const rng = xoroshiro128plus((seed ^ Math.imul(index + 1, 0x9e3779b1)) | 0);
    const spec = pick(rng, config.providers);
    const profile = pick(rng, config.profiles);
    const pid = uniformInt(rng, 1000, 99_999);
    const startedAt = this.now - uniformInt(rng, 60_000, 3 * 3_600_000);
    const status = applySignal(undefined, { type: "SessionStarted" }, startedAt) as AgentStatus;
    const cwd = `/Users/dev/${projectFor(index)}`;
    const state: AgentState = {
      id: `${spec.provider}:${pid}:${startedAt}`,
      provider: spec.provider,
      pid,
      cwd,
      startedAt,
      status: { kind: status.kind, reason: status.reason, since: status.since },
      lastEventAt: startedAt,
      project: cwd,
    };
    // Solo Claude da título, actividad y último pedido (ADR 0009). Sin rng: no cambia la flota.
    if (spec.provider === "claude") {
      state.title = config.titles[(index * 3) % config.titles.length] as string;
    }
    return {
      state,
      index,
      turns: 0,
      rng,
      profile,
      tools: spec.tools,
      toolsLeft: 0,
      toolOpen: false,
      permissionPending: false,
      nextAt: this.now + uniformInt(rng, 0, profile.meanIdleMs),
    };
  }

  snapshot(): AgentState[] {
    return [...this.agents.values()].map((a) => a.state);
  }

  /** Marca el agente para salir en el siguiente paso. Devuelve false si no existe. */
  kill(id: AgentId): boolean {
    if (!this.agents.has(id) || this.pendingKills.has(id)) return false;
    this.pendingKills.add(id);
    return true;
  }

  /** Aplica los kills pendientes sin avanzar el reloj. */
  flushKills(): AgentId[] {
    const removed = [...this.pendingKills];
    for (const id of removed) this.agents.delete(id);
    this.pendingKills.clear();
    return removed;
  }

  /** Avanza un tick virtual y devuelve solo lo que cambió. */
  step(): FleetStep {
    const removed = this.flushKills();
    this.now += TICK_MS;
    const upserts: AgentState[] = [];
    for (const [id, agent] of this.agents) {
      if (agent.nextAt > this.now) continue;
      const signal = this.nextSignal(agent);
      const next = applySignal(toStatus(agent.state), signal, this.now);
      if (next === "removed") {
        this.agents.delete(id);
        removed.push(id);
        continue;
      }
      agent.state = withInfo(withStatus(agent.state, next, this.now), agent, signal);
      upserts.push(agent.state);
    }
    return { upserts, removed };
  }

  /** Modelo de turnos: prompt → herramientas (algunas piden permiso) → stop. */
  private nextSignal(a: SimAgent): Signal {
    const { rng, profile: p } = a;
    const after = (ms: number) => {
      a.nextAt = this.now + ms;
    };
    switch (a.state.status.kind) {
      case "dead":
        after(Number.POSITIVE_INFINITY);
        return { type: "ProcessGone" };
      case "idle":
      case "error":
        a.toolsLeft = 1 + Math.floor(-p.meanToolsPerTurn * Math.log(1 - uniformFloat64(rng)));
        after(expMs(rng, p.meanThinkMs));
        return { type: "PromptSubmitted" };
      case "waiting":
        a.toolOpen = false;
        a.toolsLeft--;
        after(expMs(rng, p.meanThinkMs));
        return { type: "ToolFinished" };
      case "working": {
        if (a.toolOpen && a.permissionPending) {
          a.permissionPending = false;
          after(expMs(rng, p.meanPermissionWaitMs));
          return { type: "AwaitingInput" };
        }
        if (a.toolOpen) {
          a.toolOpen = false;
          a.toolsLeft--;
          after(expMs(rng, p.meanThinkMs));
          return { type: "ToolFinished", failed: chance(rng, 0.1) };
        }
        if (a.toolsLeft <= 0) {
          after(expMs(rng, p.meanIdleMs));
          return { type: "Stopped" };
        }
        if (chance(rng, p.crashProbPerTurn / p.meanToolsPerTurn)) {
          after(Number.POSITIVE_INFINITY);
          return { type: "ProcessGone" };
        }
        if (chance(rng, p.errorProbPerTurn / p.meanToolsPerTurn)) {
          after(expMs(rng, p.meanIdleMs));
          return { type: "Error" };
        }
        a.toolOpen = true;
        a.permissionPending = chance(rng, p.permissionProb);
        after(a.permissionPending ? expMs(rng, p.meanThinkMs / 4) : expMs(rng, p.meanToolMs));
        return {
          type: "ToolStarted",
          tool: a.tools[uniformInt(rng, 0, a.tools.length - 1)] as string,
        };
      }
    }
  }
}

/** Actividad y último pedido de Claude según la señal, como harían sus hooks. */
function withInfo(s: AgentState, a: SimAgent, signal: Signal): AgentState {
  if (s.provider !== "claude") return s;
  switch (signal.type) {
    case "PromptSubmitted": {
      a.turns++;
      const prompt = config.prompts[(a.index + a.turns) % config.prompts.length] as string;
      return { ...s, lastPrompt: prompt };
    }
    case "ToolStarted":
      return { ...s, activity: config.activities[signal.tool] ?? signal.tool };
    case "Stopped": {
      const { activity: _, ...rest } = s;
      return rest;
    }
    default:
      return s;
  }
}

function toStatus(s: AgentState): AgentStatus {
  const { kind, reason, since } = s.status;
  return s.lastTool === undefined
    ? { kind, reason, since }
    : { kind, reason, since, lastTool: s.lastTool };
}

function withStatus(s: AgentState, st: AgentStatus, now: number): AgentState {
  const next: AgentState = {
    ...s,
    status: { kind: st.kind, reason: st.reason, since: st.since },
    lastEventAt: now,
  };
  if (st.lastTool !== undefined) next.lastTool = st.lastTool;
  return next;
}
