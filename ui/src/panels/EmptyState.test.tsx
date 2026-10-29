// @vitest-environment jsdom
import { act, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AgentState, Provider } from "../protocol";
import { createKitiStore } from "../state/store";
import { EmptyState } from "./EmptyState";

const sampleAgent: AgentState = {
  id: "claude:1:2",
  provider: "claude",
  pid: 100,
  cwd: "/Users/dev/app",
  project: "/Users/dev/app",
  startedAt: 1000,
  status: { kind: "working", reason: "hook", since: 1000 },
  lastEventAt: 1000,
  history: [],
};

describe("EmptyState", () => {
  it("no se muestra antes de recibir el primer Snapshot", () => {
    const store = createKitiStore();
    render(<EmptyState store={store} />);
    expect(screen.queryByText("Sin agentes en ejecución")).toBeNull();
  });

  it("se muestra cuando el Snapshot llega vacío y sin conversaciones apagadas", () => {
    const store = createKitiStore();
    store.getState().receive({ type: "Snapshot", seq: 0, agents: [] });
    render(<EmptyState store={store} />);
    expect(screen.getByText("Sin agentes en ejecución")).toBeTruthy();
  });

  it("se oculta cuando hay agentes o conversaciones apagadas", () => {
    const store = createKitiStore();
    store.getState().receive({ type: "Snapshot", seq: 0, agents: [sampleAgent] });
    render(<EmptyState store={store} />);
    expect(screen.queryByText("Sin agentes en ejecución")).toBeNull();

    act(() => {
      store.getState().receive({ type: "Snapshot", seq: 1, agents: [] });
      store.getState().setConversations([
        {
          sessionId: "s1",
          provider: "claude",
          cwd: "/Users/dev/app",
          project: "/Users/dev/app",
          updatedAt: 2000,
        },
      ]);
    });
    expect(screen.queryByText("Sin agentes en ejecución")).toBeNull();
  });

  it("ofrece botones para lanzar un agente cuando hay terminales", async () => {
    const { default: userEvent } = await import("@testing-library/user-event");
    const store = createKitiStore();
    store.getState().receive({ type: "Snapshot", seq: 0, agents: [] });
    const onLaunch = vi.fn<(p: Provider) => void>();
    render(<EmptyState store={store} onLaunch={onLaunch} />);
    await userEvent.click(screen.getByRole("button", { name: "+ Claude Code" }));
    expect(onLaunch).toHaveBeenCalledWith("claude");
  });
});
