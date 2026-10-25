// @vitest-environment jsdom
import { act, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { AgentState } from "../protocol";
import { createKitiStore } from "../state/store";
import { AgentPanel } from "./AgentPanel";

const NOW = 1_000_000;

function setup(agent: AgentState, terminalsAvailable = false) {
  const store = createKitiStore();
  store.getState().receive({ type: "Snapshot", seq: 0, agents: [agent] });
  store.getState().select(agent.id);
  render(
    <AgentPanel
      store={store}
      world={() => undefined}
      now={() => NOW}
      terminalsAvailable={terminalsAvailable}
    />,
  );
  return store;
}

const base: AgentState = {
  id: "claude:1:2",
  provider: "claude",
  pid: 4242,
  cwd: "/Users/dev/billing",
  startedAt: NOW - 3_600_000,
  status: { kind: "working", reason: "hook", since: NOW - 90_000 },
  lastTool: "Edit",
  lastEventAt: NOW - 4_000,
};

describe("AgentPanel", () => {
  it("muestra estado, motivo, herramienta y último evento", () => {
    setup(base);
    expect(screen.getByText("billing")).toBeTruthy();
    expect(screen.getByText(/trabajando · hace 2 min/)).toBeTruthy();
    expect(screen.getByText("hook del agente")).toBeTruthy();
    expect(screen.getByText("Edit")).toBeTruthy();
    expect(screen.getByText("hace 4 s")).toBeTruthy();
  });

  it("explica de dónde sale cada estado", () => {
    setup({
      ...base,
      status: { kind: "idle", reason: "stale", since: NOW - 31_000 },
    });
    expect(screen.getByText(/inactivo/)).toBeTruthy();
    expect(screen.getByText("sin actividad reciente")).toBeTruthy();
  });

  it("un agente recién detectado sin hooks lo dice", () => {
    setup({ ...base, status: { kind: "idle", reason: "startup", since: NOW } });
    expect(screen.getByText("detectado, aún sin eventos")).toBeTruthy();
  });

  it("un agente abierto fuera de Kiti avisa de que su terminal no se puede mostrar", () => {
    setup(base, true);
    expect(screen.getByText(/abierto fuera de Kiti/i)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Abrir terminal" })).toBeNull();
  });

  it("un agente lanzado desde Kiti ofrece abrir su terminal", async () => {
    const { default: userEvent } = await import("@testing-library/user-event");
    const store = setup(base, true);
    act(() =>
      store
        .getState()
        .setTerminals([
          { id: "pty-7", pid: base.pid, provider: "claude", cwd: base.cwd, running: true },
        ]),
    );
    await userEvent.click(screen.getByRole("button", { name: "Abrir terminal" }));
    expect(store.getState().openTerminalId).toBe("pty-7");
  });

  it("en el simulador no hay aviso de terminal", () => {
    setup(base, false);
    expect(screen.queryByText(/abierto fuera de Kiti/i)).toBeNull();
  });
});
