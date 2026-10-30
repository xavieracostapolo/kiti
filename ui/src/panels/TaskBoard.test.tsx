// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { TaskInfo } from "../protocol";
import { createKitiStore } from "../state/store";
import { TaskBoard, TaskChip } from "./TaskBoard";

afterEach(cleanup);

const NOW = 100 * 3_600_000;
const MIN = 60_000;

const task = (over: Partial<TaskInfo>): TaskInfo => ({
  id: 1,
  alias: "claude-2",
  orchestrator: "claude-1",
  role: "implementar",
  provider: "claude",
  model: "sonnet",
  branch: "feat/x",
  state: "running",
  createdAt: NOW - 5 * MIN,
  ...over,
});

const TASKS: TaskInfo[] = [
  task({
    id: 1,
    state: "done",
    createdAt: NOW - 60 * MIN,
    finishedAt: NOW - 30 * MIN,
    pr: "https://github.com/ejemplo/tienda-web/pull/12",
    branch: "feat/hecha",
  }),
  task({ id: 2, state: "running", createdAt: NOW - 10 * MIN, branch: "feat/en-curso" }),
  task({
    id: 3,
    state: "failed",
    createdAt: NOW - 90 * MIN,
    finishedAt: NOW - 80 * MIN,
    reason: "el agente terminó con error",
    branch: "feat/fallida",
  }),
  task({ id: 4, state: "stuck", createdAt: NOW - 20 * MIN, branch: "feat/atascada" }),
];

function setup(tasks: TaskInfo[] = TASKS) {
  const store = createKitiStore();
  render(
    <>
      <TaskChip store={store} />
      <TaskBoard store={store} now={() => NOW} />
    </>,
  );
  act(() => store.getState().setTasks(tasks));
  return store;
}

describe("TaskChip", () => {
  it("sin tareas no se muestra", () => {
    setup([]);
    expect(screen.queryByRole("button", { name: /tarea/i })).toBeNull();
  });

  it("cuenta las abiertas y se vuelve ámbar con una atascada o fallida", () => {
    setup();
    const chip = screen.getByRole("button", { name: /tareas/ });
    expect(chip.textContent).toBe("⚙ 3 tareas");
    expect(chip.getAttribute("data-alert")).toBe("true");
  });

  it("sin atascadas ni fallidas no es ámbar", () => {
    setup([task({ id: 1 })]);
    const chip = screen.getByRole("button", { name: /tarea/ });
    expect(chip.textContent).toBe("⚙ 1 tarea");
    expect(chip.getAttribute("data-alert")).toBeNull();
  });
});

describe("TaskBoard", () => {
  it("cerrado no pinta nada; el chip lo abre", () => {
    setup();
    expect(screen.queryByRole("complementary", { name: "Tablero de tareas" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /tareas/ }));
    expect(screen.getByRole("complementary", { name: "Tablero de tareas" })).toBeTruthy();
  });

  it("una fila por tarea: vivas primero y luego las terminadas, de la más nueva a la más vieja", () => {
    const store = setup();
    act(() => store.getState().setTasksOpen(true));
    const rows = within(screen.getByRole("list", { name: "Tareas" })).getAllByRole("listitem");
    expect(rows.map((r) => r.getAttribute("data-state"))).toEqual([
      "running",
      "stuck",
      "done",
      "failed",
    ]);
    expect(rows[0]?.textContent).toContain("feat/en-curso");
    expect(rows[0]?.textContent).toContain("claude-2");
    expect(rows[0]?.textContent).toContain("implementar");
    expect(rows[0]?.textContent).toContain("sonnet");
    expect(rows[0]?.textContent).toContain("hace 10 min");
  });

  it("muestra el PR como enlace y el motivo de las fallidas", () => {
    const store = setup();
    act(() => store.getState().setTasksOpen(true));
    const link = screen.getByRole("link", { name: /PR #12/ });
    expect(link.getAttribute("href")).toBe("https://github.com/ejemplo/tienda-web/pull/12");
    expect(screen.getByText("el agente terminó con error")).toBeTruthy();
  });
});
