import { describe, expect, it } from "vitest";
import { fakeTasks } from "./fakeTasks";
import { SimFleet } from "./fleet";

const agents = new SimFleet({ count: 20, seed: 42 }).snapshot();
const NOW = 1_000_000_000;

describe("fakeTasks", () => {
  it("una tarea de cada estado, con ids únicos", () => {
    const tasks = fakeTasks(agents, NOW);
    expect(tasks.map((t) => t.state).sort()).toEqual([
      "done",
      "failed",
      "launching",
      "running",
      "stuck",
    ]);
    expect(new Set(tasks.map((t) => t.id)).size).toBe(tasks.length);
  });

  it("las fallidas y atascadas traen motivo, y la hecha trae PR", () => {
    const tasks = fakeTasks(agents, NOW);
    for (const t of tasks.filter((x) => x.state === "failed" || x.state === "stuck")) {
      expect(t.reason).toBeTruthy();
    }
    expect(tasks.find((t) => t.state === "done")?.pr).toMatch(/\/pull\/\d+$/);
  });

  it("el alias concuerda con el proveedor de una oficina real", () => {
    const tasks = fakeTasks(agents, NOW);
    const prefix = { claude: "claude", codex: "codex", gemini: "gemini", antigravity: "agy" };
    for (const t of tasks) {
      if (t.alias) expect(t.alias.startsWith(`${prefix[t.provider]}-`)).toBe(true);
    }
    expect(tasks.every((t) => t.createdAt <= NOW)).toBe(true);
  });

  it("sin agentes no hay alias y el orquestador cae a claude-1", () => {
    const tasks = fakeTasks([], NOW);
    expect(tasks).toHaveLength(5);
    expect(tasks.every((t) => t.alias === undefined)).toBe(true);
    expect(tasks.every((t) => t.orchestrator === "claude-1")).toBe(true);
  });
});
