// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { AgentState } from "../protocol";
import { createKitiStore } from "../state/store";
import { CloseOfficeDialog } from "./CloseOfficeDialog";

const agent: AgentState = {
  id: "claude:1:2",
  provider: "claude",
  pid: 4242,
  cwd: "/Users/dev/billing",
  project: "/Users/dev/billing",
  startedAt: 0,
  status: { kind: "working", reason: "hook", since: 0 },
  lastEventAt: 0,
  history: [],
};

function setup() {
  const store = createKitiStore();
  store.getState().receive({ type: "Snapshot", seq: 0, agents: [agent] });
  store.getState().requestKill(agent.id);
  const killAgent = vi.fn(async () => ({ kind: "killed" as const }));
  render(<CloseOfficeDialog store={store} killAgent={killAgent} />);
  return { store, killAgent };
}

describe("CloseOfficeDialog", () => {
  it("muestra el agente y su PID", () => {
    setup();
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    expect(screen.getByText(/billing/)).toBeTruthy();
    expect(screen.getByText(/4242/)).toBeTruthy();
  });

  it("al confirmar llama a killAgent una vez y cierra", async () => {
    const { store, killAgent } = setup();
    await userEvent.click(screen.getByRole("button", { name: /cerrar oficina/i }));
    expect(killAgent).toHaveBeenCalledExactlyOnceWith(agent.id);
    expect(store.getState().view.pendingKill).toBeUndefined();
  });

  it("al cancelar no llama a killAgent", async () => {
    const { store, killAgent } = setup();
    await userEvent.click(screen.getByRole("button", { name: /cancelar/i }));
    expect(killAgent).not.toHaveBeenCalled();
    expect(store.getState().view.pendingKill).toBeUndefined();
  });

  it("sin kill pendiente no se muestra", () => {
    const store = createKitiStore();
    render(<CloseOfficeDialog store={store} killAgent={vi.fn()} />);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });
});
