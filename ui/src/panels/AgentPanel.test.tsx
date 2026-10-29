// @vitest-environment jsdom
import { act, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AgentState, FocusResult } from "../protocol";
import { createKitiStore } from "../state/store";
import { AgentPanel } from "./AgentPanel";

const NOW = 1_000_000;

function setup(
  agent: AgentState,
  terminalsAvailable = false,
  onFocusExternal?: (id: string) => Promise<FocusResult>,
) {
  const store = createKitiStore();
  store.getState().receive({ type: "Snapshot", seq: 0, agents: [agent] });
  store.getState().select(agent.id);
  render(
    <AgentPanel
      store={store}
      world={() => undefined}
      now={() => NOW}
      terminalsAvailable={terminalsAvailable}
      {...(onFocusExternal ? { onFocusExternal } : {})}
    />,
  );
  return store;
}

const base: AgentState = {
  id: "claude:1:2",
  provider: "claude",
  pid: 4242,
  cwd: "/Users/dev/billing",
  project: "/Users/dev/billing",
  startedAt: NOW - 3_600_000,
  status: { kind: "working", reason: "hook", since: NOW - 90_000 },
  lastTool: "Edit",
  lastEventAt: NOW - 4_000,
  history: [],
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

  it("con la terminal abierta el panel de detalle se oculta", () => {
    const store = setup(base, true);
    act(() => {
      store
        .getState()
        .setTerminals([
          { id: "pty-7", pid: base.pid, provider: "claude", cwd: base.cwd, running: true },
        ]);
      store.getState().openTerminal("pty-7");
    });
    expect(screen.queryByRole("complementary", { name: "Agente seleccionado" })).toBeNull();
  });

  it("un agente abierto fuera de Kiti lleva a su terminal y dice dónde estaba", async () => {
    const { default: userEvent } = await import("@testing-library/user-event");
    const focus = vi.fn(async (): Promise<FocusResult> => ({ kind: "focused", app: "Ghostty" }));
    setup(base, true, focus);
    await userEvent.click(screen.getByRole("button", { name: "Ir a su terminal" }));
    expect(focus).toHaveBeenCalledWith(base.id);
    expect((await screen.findByRole("status")).textContent).toContain("Abierto en Ghostty");
  });

  it("si no sabe la pestaña o la terminal, lo dice", async () => {
    const { default: userEvent } = await import("@testing-library/user-event");
    setup(base, true, async () => ({ kind: "appOnly", app: "Visual Studio Code" }));
    await userEvent.click(screen.getByRole("button", { name: "Ir a su terminal" }));
    expect((await screen.findByRole("status")).textContent).toContain("no supe cuál es su pestaña");
  });

  it("muestra git del proyecto, modelo, contexto y coste de la sesión", () => {
    const store = setup({
      ...base,
      model: "Opus 4.7",
      contextPct: 34,
      costCents: 185,
    });
    act(() =>
      store.getState().receive({
        type: "Git",
        project: "/Users/dev/billing",
        git: { branch: "feat/board", changed: 3, ahead: 1, behind: 2 },
      }),
    );
    expect(screen.getByText("feat/board · 3 cambios · ↑1 · ↓2")).toBeTruthy();
    expect(screen.getByText("Opus 4.7")).toBeTruthy();
    expect(screen.getByText("34 %")).toBeTruthy();
    expect(screen.getByText("$1.85 sesión")).toBeTruthy();
  });

  it("el botón de cerrar deselecciona la oficina", async () => {
    const { default: userEvent } = await import("@testing-library/user-event");
    const store = setup(base);
    await userEvent.click(screen.getByTitle("Cerrar panel"));
    expect(store.getState().view.selectedId).toBeUndefined();
  });

  it("muestra objetivo inicial, tareas, archivos tocados y memoria del proyecto", () => {
    const store = setup({
      ...base,
      initialPrompt: "Implementar memoria en SQLite",
      lastPrompt: "sí, continúa",
      touchedFiles: ["memory.rs", "memory_sqlite.rs"],
      tasks: [
        { text: "Dominio puro", status: "completed" },
        { text: "Adaptador SQLite FTS5", status: "inProgress" },
      ],
    });
    act(() =>
      store.getState().receive({
        type: "Memory",
        memories: [
          {
            id: 1,
            project: "/Users/dev/billing",
            kind: "decision",
            title: "Usar SQLite FTS5 BM25",
            detail: "Por latencia < 1 ms",
            files: ["memory_sqlite.rs"],
            author: "agy-2",
            createdAt: NOW,
          },
        ],
      }),
    );
    expect(screen.getByText("Implementar memoria en SQLite")).toBeTruthy();
    expect(screen.getByText("sí, continúa")).toBeTruthy();
    expect(screen.getByText("1/2 hechas · Adaptador SQLite FTS5")).toBeTruthy();
    expect(screen.getByText("memory.rs, memory_sqlite.rs")).toBeTruthy();
    expect(screen.getByText("[decision] Usar SQLite FTS5 BM25")).toBeTruthy();
  });
});
