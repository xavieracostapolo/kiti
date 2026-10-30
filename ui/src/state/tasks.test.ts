import { describe, expect, it } from "vitest";
import { sortTasks, type TaskInfo, taskSummary } from "./tasks";

const H = 3_600_000;
const NOW = 100 * H;

const task = (over: Partial<TaskInfo>): TaskInfo => ({
  id: 1,
  orchestrator: "claude-1",
  role: "implementar",
  provider: "claude",
  model: "sonnet",
  branch: "feat/x",
  state: "running",
  createdAt: NOW - H,
  ...over,
});

describe("taskSummary", () => {
  it("sin tareas todo es cero", () => {
    expect(taskSummary([], NOW)).toEqual({ active: 0, attention: 0, doneLast24h: 0 });
  });

  it("cuenta en curso, atascadas o fallidas y hechas en 24 h", () => {
    const tasks = [
      task({ id: 1, state: "launching" }),
      task({ id: 2, state: "running" }),
      task({ id: 3, state: "stuck" }),
      task({ id: 4, state: "failed", finishedAt: NOW - H }),
      task({ id: 5, state: "done", finishedAt: NOW - 2 * H }),
      task({ id: 6, state: "done", finishedAt: NOW - 25 * H }),
    ];
    expect(taskSummary(tasks, NOW)).toEqual({ active: 2, attention: 2, doneLast24h: 1 });
  });
});

describe("sortTasks", () => {
  it("primero las vivas y luego las terminadas, de la más nueva a la más vieja", () => {
    const tasks = [
      task({ id: 1, state: "done", createdAt: 5 }),
      task({ id: 2, state: "running", createdAt: 1 }),
      task({ id: 3, state: "stuck", createdAt: 3 }),
      task({ id: 4, state: "failed", createdAt: 9 }),
    ];
    expect(sortTasks(tasks).map((t) => t.id)).toEqual([3, 2, 4, 1]);
  });
});
