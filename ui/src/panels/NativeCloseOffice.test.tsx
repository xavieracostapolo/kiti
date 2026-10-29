// @vitest-environment jsdom
import { act, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AgentState } from "../protocol";
import { createKitiStore } from "../state/store";
import { NativeCloseOffice } from "./NativeCloseOffice";

const agent: AgentState = {
  id: "claude:1:2",
  provider: "claude",
  pid: 4242,
  cwd: "/Users/dev/kiti",
  project: "/Users/dev/kiti",
  startedAt: 0,
  status: { kind: "idle", reason: "hook", since: 0 },
  lastEventAt: 0,
  history: [],
};

describe("NativeCloseOffice", () => {
  it("pedir el cierre llama una vez a kill_agent (el core enseña el diálogo nativo)", () => {
    const store = createKitiStore();
    store.getState().receive({ type: "Snapshot", seq: 0, agents: [agent] });
    const killAgent = vi.fn(async () => ({ kind: "cancelled" as const }));
    render(<NativeCloseOffice store={store} killAgent={killAgent} />);
    act(() => store.getState().requestKill(agent.id));
    expect(killAgent).toHaveBeenCalledTimes(1);
    expect(killAgent).toHaveBeenCalledWith(agent.id);
    // La petición se consume: volver a pedirlo abre otro diálogo.
    expect(store.getState().view.pendingKill).toBeUndefined();
    act(() => store.getState().requestKill(agent.id));
    expect(killAgent).toHaveBeenCalledTimes(2);
  });

  it("si el core falla, lo registra sin romper la UI", async () => {
    const store = createKitiStore();
    store.getState().receive({ type: "Snapshot", seq: 0, agents: [agent] });
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const killAgent = vi.fn(async () => {
      throw { code: "KILL_FAILED", message: "sin permiso" };
    });
    render(<NativeCloseOffice store={store} killAgent={killAgent} />);
    await act(async () => store.getState().requestKill(agent.id));
    expect(warn).toHaveBeenCalledWith(expect.stringContaining("KILL_FAILED"));
    warn.mockRestore();
  });
});
