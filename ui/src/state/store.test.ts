import { describe, expect, it } from "vitest";
import type { AgentState } from "../protocol";
import { createKitiStore } from "./store";

const a: AgentState = {
  id: "a",
  provider: "codex",
  pid: 1,
  cwd: "/w/a",
  startedAt: 0,
  status: { kind: "idle", reason: "hook", since: 0 },
  lastEventAt: 0,
};

describe("store", () => {
  it("receive aplica mensajes del transport", () => {
    const store = createKitiStore();
    store.getState().receive({ type: "Snapshot", seq: 0, agents: [a] });
    expect(store.getState().agents.agents.get("a")).toBe(a);
  });

  it("seleccionar y pedir kill no tocan el slice de agentes", () => {
    const store = createKitiStore();
    store.getState().receive({ type: "Snapshot", seq: 0, agents: [a] });
    const before = store.getState().agents;
    store.getState().select("a");
    store.getState().requestKill("a");
    expect(store.getState().agents).toBe(before);
    expect(store.getState().view).toEqual({ selectedId: "a", pendingKill: "a", lod: "far" });
    store.getState().cancelKill();
    expect(store.getState().view.pendingKill).toBeUndefined();
  });

  it("si el agente seleccionado desaparece, se deselecciona", () => {
    const store = createKitiStore();
    store.getState().receive({ type: "Snapshot", seq: 0, agents: [a] });
    store.getState().select("a");
    store.getState().receive({ type: "Diff", seq: 1, ingestMs: 0, upserts: [], removed: ["a"] });
    expect(store.getState().view.selectedId).toBeUndefined();
  });

  it("setLod solo notifica si cambia", () => {
    const store = createKitiStore();
    let calls = 0;
    store.subscribe(() => calls++);
    store.getState().setLod("far");
    store.getState().setLod("mid");
    expect(calls).toBe(1);
  });
});
