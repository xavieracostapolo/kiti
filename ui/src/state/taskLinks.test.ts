import { describe, expect, it } from "vitest";
import type { MailState, TaskInfo } from "../protocol";
import { clipLink, officeTaskBadges, taskLinks } from "./taskLinks";

const MAIL = {
  mode: "auto",
  paused: false,
  attention: 0,
  threads: [],
  aliases: [
    { agent: "claude:1:1", alias: "claude-1" },
    { agent: "claude:2:2", alias: "claude-2" },
    { agent: "codex:3:3", alias: "codex-1" },
  ],
} as unknown as MailState;

const task = (over: Partial<TaskInfo>): TaskInfo =>
  ({
    id: 1,
    orchestrator: "claude-1",
    alias: "claude-2",
    role: "implementar",
    state: "running",
    createdAt: 0,
    ...over,
  }) as TaskInfo;

describe("taskLinks", () => {
  it("une el orquestador con el subagente por sus alias", () => {
    expect(taskLinks([task({})], MAIL)).toEqual([
      {
        from: "claude:1:1",
        to: "claude:2:2",
        stuck: false,
        fromAlias: "claude-1",
        role: "implementar",
      },
    ]);
  });

  it("marca las atascadas e incluye las que están lanzándose", () => {
    const links = taskLinks(
      [task({ state: "stuck" }), task({ id: 2, state: "launching", alias: "codex-1" })],
      MAIL,
    );
    expect(links.map((l) => l.stuck)).toEqual([true, false]);
  });

  it("ignora las terminadas, las sin alias y las de alias sin oficina", () => {
    const tasks = [
      task({ state: "done" }),
      task({ state: "failed" }),
      task({ alias: null }),
      task({ alias: "agy-9" }),
      task({ orchestrator: "agy-9" }),
    ];
    expect(taskLinks(tasks, MAIL)).toEqual([]);
  });

  it("sin mensajes no hay líneas", () => {
    expect(taskLinks([task({})], undefined)).toEqual([]);
  });
});

describe("officeTaskBadges", () => {
  it("da al subagente quién lo lanzó y al orquestador cuántos tiene", () => {
    const links = taskLinks(
      [task({}), task({ id: 2, alias: "codex-1", role: "documentar" })],
      MAIL,
    );
    const badges = officeTaskBadges(links);
    expect(badges.get("claude:1:1")).toEqual({ launched: 2 });
    expect(badges.get("claude:2:2")).toEqual({ launched: 0, by: "claude-1", role: "implementar" });
    expect(badges.get("codex:3:3")).toEqual({ launched: 0, by: "claude-1", role: "documentar" });
  });

  it("un agente puede ser a la vez subagente y orquestador", () => {
    const links = taskLinks(
      [task({}), task({ id: 2, orchestrator: "claude-2", alias: "codex-1" })],
      MAIL,
    );
    expect(officeTaskBadges(links).get("claude:2:2")).toEqual({
      launched: 1,
      by: "claude-1",
      role: "implementar",
    });
  });
});

describe("clipLink", () => {
  const A = { x: 0, y: 0, w: 10, h: 10 };
  const B = { x: 30, y: 0, w: 10, h: 10 };

  it("sale por el borde del orquestador y llega al borde del subagente", () => {
    const s = clipLink(A, B);
    expect(s).toBeDefined();
    expect(s?.x1).toBeCloseTo(10);
    expect(s?.y1).toBeCloseTo(5);
    expect(s?.x2).toBeCloseTo(30);
    expect(s?.y2).toBeCloseTo(5);
  });

  it("la flecha apunta del orquestador al subagente", () => {
    expect(clipLink(A, B)?.angle).toBeCloseTo(0);
    expect(clipLink(B, A)?.angle).toBeCloseTo(Math.PI);
    expect(clipLink(A, { x: 0, y: 30, w: 10, h: 10 })?.angle).toBeCloseTo(Math.PI / 2);
  });

  it("en diagonal los extremos quedan sobre los bordes", () => {
    const s = clipLink(A, { x: 30, y: 30, w: 10, h: 10 });
    expect(s?.x1).toBeCloseTo(10);
    expect(s?.y1).toBeCloseTo(10);
    expect(s?.x2).toBeCloseTo(30);
    expect(s?.y2).toBeCloseTo(30);
  });

  it("no dibuja si las oficinas se tocan o se solapan", () => {
    expect(clipLink(A, { x: 10, y: 0, w: 10, h: 10 })).toBeUndefined();
    expect(clipLink(A, { x: 5, y: 5, w: 10, h: 10 })).toBeUndefined();
  });
});
