// Reducer puro del stream del transport (ADR 0003 §4). El semáforo llega calculado: la UI no infiere.

import type { AgentId, AgentState, ServerMsg, StatusKind } from "../protocol";

export type AgentsState = {
  seq: number;
  agents: ReadonlyMap<AgentId, AgentState>;
  /** Hubo un hueco en `seq`: hay que reconectar para recibir un Snapshot nuevo. */
  resync: boolean;
};

export const EMPTY_AGENTS: AgentsState = { seq: -1, agents: new Map(), resync: false };

export function applyMsg(state: AgentsState, msg: ServerMsg): AgentsState {
  switch (msg.type) {
    case "Snapshot":
      return { seq: msg.seq, agents: new Map(msg.agents.map((a) => [a.id, a])), resync: false };
    case "Diff": {
      if (msg.seq !== state.seq + 1) return { ...state, resync: true };
      const agents = new Map(state.agents);
      for (const a of msg.upserts) agents.set(a.id, a);
      for (const id of msg.removed) agents.delete(id);
      return { seq: msg.seq, agents, resync: false };
    }
    case "Metrics":
    case "Focus":
      return state;
  }
}

export type SceneOp =
  | { op: "add"; agent: AgentState }
  | { op: "update"; agent: AgentState }
  | { op: "remove"; id: AgentId };

/** Operaciones mínimas para pasar de `prev` a `next`, comparando referencias. O(n). */
export function planScene(
  prev: ReadonlyMap<AgentId, AgentState>,
  next: ReadonlyMap<AgentId, AgentState>,
): SceneOp[] {
  if (prev === next) return [];
  const ops: SceneOp[] = [];
  for (const id of prev.keys()) if (!next.has(id)) ops.push({ op: "remove", id });
  for (const [id, agent] of next) {
    const old = prev.get(id);
    if (old === undefined) ops.push({ op: "add", agent });
    else if (old !== agent) ops.push({ op: "update", agent });
  }
  return ops;
}

export type Counts = Record<StatusKind, number>;

export function countsByKind(agents: ReadonlyMap<AgentId, AgentState>): Counts {
  const counts: Counts = { working: 0, waiting: 0, idle: 0, error: 0, dead: 0 };
  for (const a of agents.values()) counts[a.status.kind]++;
  return counts;
}
