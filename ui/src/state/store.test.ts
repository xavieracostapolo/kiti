import { describe, expect, it } from "vitest";
import type { AgentState, ConversationInfo } from "../protocol";
import { dormantId } from "./rooms";
import { createKitiStore } from "./store";

const a: AgentState = {
  id: "a",
  provider: "codex",
  pid: 1,
  cwd: "/w/a",
  project: "/w/a",
  startedAt: 0,
  status: { kind: "idle", reason: "hook", since: 0 },
  lastEventAt: 0,
  history: [],
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
      mailsTotal: 0,
      mailPending: 0,
      memoriesTotal: 0,
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

  it("las conversaciones cerradas son oficinas apagadas hasta que su agente se enciende", () => {
    const store = createKitiStore();
    const conv: ConversationInfo = {
      sessionId: "s1",
      provider: "claude",
      cwd: "/w/a",
      project: "/w/a",
      updatedAt: 0,
    };
    store.getState().receive({ type: "Snapshot", seq: 0, agents: [a] });
    store.getState().setConversations([conv]);
    expect([...store.getState().dormant.keys()]).toEqual([dormantId("s1")]);
    // Seleccionar una apagada no la pierde con el siguiente Diff.
    store.getState().select(dormantId("s1"));
    store.getState().receive({ type: "Diff", seq: 1, ingestMs: 0, upserts: [a], removed: [] });
    expect(store.getState().view.selectedId).toBe(dormantId("s1"));
    // Al retomarla, su agente trae la sesión: la apagada desaparece y se deselecciona.
    const resumed = { ...a, id: "b", sessionId: "s1" };
    store
      .getState()
      .receive({ type: "Diff", seq: 2, ingestMs: 0, upserts: [resumed], removed: [] });
    expect(store.getState().dormant.size).toBe(0);
    expect(store.getState().view.selectedId).toBeUndefined();
  });

  it("un Focus del core (clic en un aviso) pide enfocar esa oficina", () => {
    const store = createKitiStore();
    store.getState().receive({ type: "Snapshot", seq: 0, agents: [a] });
    store.getState().openTerminal("kiti-1");
    store.getState().receive({ type: "Focus", id: "a" });
    expect(store.getState().pendingLaunchPid).toBe(a.pid);
    expect(store.getState().openTerminalId).toBeUndefined();
    // Un id desconocido no hace nada.
    store.getState().setPendingLaunch(undefined);
    store.getState().receive({ type: "Focus", id: "nadie" });
    expect(store.getState().pendingLaunchPid).toBeUndefined();
  });

  it("permite cambiar el modo de layout del mapa", () => {
    const store = createKitiStore();
    expect(store.getState().layoutMode).toBe("dynamic");
    store.getState().setLayoutMode("vertical");
    expect(store.getState().layoutMode).toBe("vertical");
  });

  it("acota el ancho de la terminal entre el mínimo y el máximo", () => {
    const store = createKitiStore();
    expect(store.getState().terminalFraction).toBe(0.46);
    store.getState().setTerminalFraction(0.9);
    expect(store.getState().terminalFraction).toBe(0.72);
    store.getState().setTerminalFraction(0.1);
    expect(store.getState().terminalFraction).toBe(0.32);
  });
});
