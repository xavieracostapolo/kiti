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

  it("guarda la fuente de datos del Hello para avisar si es simulada", () => {
    const store = createKitiStore();
    expect(store.getState().source).toBeUndefined();
    store.getState().setSource("sim");
    expect(store.getState().source).toBe("sim");
  });

  it("guarda las CoreMetrics del stream de debug sin tocar los agentes", () => {
    const store = createKitiStore();
    store.getState().receive({ type: "Snapshot", seq: 0, agents: [a] });
    const agents = store.getState().agents;
    store.getState().receive({
      type: "Metrics",
      agents: 1,
      eventsPerSec: 3,
      hookToPublishMsP95: 1,
      rssBytes: 32_000_000,
      cpuPercent: 0.1,
    });
    expect(store.getState().agents).toBe(agents);
    expect(store.getState().coreMetrics?.rssBytes).toBe(32_000_000);
  });

  it("une cada terminal con su oficina por pid y abre la del agente seleccionado", () => {
    const store = createKitiStore();
    store.getState().receive({ type: "Snapshot", seq: 0, agents: [a] });
    expect(store.getState().terminalFor(a.pid)).toBeUndefined();
    store
      .getState()
      .setTerminals([{ id: "pty-1", pid: a.pid, provider: "codex", cwd: "/w/a", running: true }]);
    expect(store.getState().terminalFor(a.pid)?.id).toBe("pty-1");
    store.getState().openTerminal("pty-1");
    expect(store.getState().openTerminalId).toBe("pty-1");
    store.getState().closeTerminal();
    expect(store.getState().openTerminalId).toBeUndefined();
  });

  it("el hover solo notifica cuando cambia de oficina", () => {
    const store = createKitiStore();
    let calls = 0;
    store.subscribe(() => calls++);
    store.getState().setHover({ id: "a", x: 10, y: 20 });
    store.getState().setHover({ id: "a", x: 12, y: 21 });
    store.getState().setHover(undefined);
    expect(calls).toBe(2);
  });
});
