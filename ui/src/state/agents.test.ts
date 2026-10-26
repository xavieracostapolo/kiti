import { describe, expect, it } from "vitest";
import type { AgentState, ServerMsg, StatusKind } from "../protocol";
import { type AgentsState, applyMsg, countsByKind, EMPTY_AGENTS, planScene } from "./agents";

function agent(id: string, kind: StatusKind = "idle"): AgentState {
  return {
    id,
    provider: "claude",
    pid: 1,
    cwd: `/w/${id}`,
    project: `/w/${id}`,
    startedAt: 0,
    status: { kind, reason: "hook", since: 0 },
    lastEventAt: 0,
    history: [],
  };
}

const snapshot = (...agents: AgentState[]): ServerMsg => ({ type: "Snapshot", seq: 0, agents });
const diff = (seq: number, upserts: AgentState[], removed: string[] = []): ServerMsg => ({
  type: "Diff",
  seq,
  ingestMs: 0,
  upserts,
  removed,
});

describe("applyMsg", () => {
  it("Snapshot reemplaza todo y fija seq", () => {
    const s = applyMsg(EMPTY_AGENTS, snapshot(agent("a"), agent("b")));
    expect([...s.agents.keys()]).toEqual(["a", "b"]);
    expect(s.seq).toBe(0);
    expect(s.resync).toBe(false);
  });

  it("Diff aplica upserts y removed, y conserva la referencia de lo que no cambia", () => {
    const s0 = applyMsg(EMPTY_AGENTS, snapshot(agent("a"), agent("b"), agent("c")));
    const s1 = applyMsg(s0, diff(1, [agent("b", "working")], ["c"]));
    expect(s1.agents.get("a")).toBe(s0.agents.get("a"));
    expect(s1.agents.get("b")?.status.kind).toBe("working");
    expect(s1.agents.has("c")).toBe(false);
    expect(s1.seq).toBe(1);
  });

  it("un hueco en seq no aplica el diff y pide resync", () => {
    const s0 = applyMsg(EMPTY_AGENTS, snapshot(agent("a")));
    const s1 = applyMsg(s0, diff(2, [agent("a", "error")]));
    expect(s1.resync).toBe(true);
    expect(s1.agents).toBe(s0.agents);
  });

  it("Metrics no toca los agentes", () => {
    const s0 = applyMsg(EMPTY_AGENTS, snapshot(agent("a")));
    const metrics: ServerMsg = {
      type: "Metrics",
      agents: 1,
      eventsPerSec: 0,
      hookToPublishMsP95: 0,
      rssBytes: 0,
      cpuPercent: 0,
    };
    expect(applyMsg(s0, metrics)).toBe(s0);
  });
});

describe("planScene", () => {
  const map = (...agents: AgentState[]) => new Map(agents.map((a) => [a.id, a]));

  it("calcula altas, bajas y cambios comparando referencias", () => {
    const a = agent("a");
    const b = agent("b");
    const prev = map(a, b);
    const b2 = agent("b", "waiting");
    const c = agent("c");
    const next = map(a, b2, c);
    expect(planScene(prev, next)).toEqual([
      { op: "update", agent: b2 },
      { op: "add", agent: c },
    ]);
    expect(planScene(next, map(a))).toEqual([
      { op: "remove", id: "b" },
      { op: "remove", id: "c" },
    ]);
  });

  it("sin cambios no hay operaciones", () => {
    const prev = map(agent("a"));
    expect(planScene(prev, prev)).toEqual([]);
  });
});

describe("countsByKind", () => {
  it("cuenta los cinco colores", () => {
    const s: AgentsState = applyMsg(
      EMPTY_AGENTS,
      snapshot(agent("a", "working"), agent("b", "working"), agent("c", "dead")),
    );
    expect(countsByKind(s.agents)).toEqual({ working: 2, waiting: 0, idle: 0, error: 0, dead: 1 });
  });
});
