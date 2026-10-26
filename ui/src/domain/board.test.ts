import { describe, expect, it } from "vitest";
import type { AgentState } from "../protocol";
import { boardLines, timeline } from "./board";

const agent = (over: Partial<AgentState> = {}): AgentState => ({
  id: "a",
  provider: "claude",
  pid: 1,
  cwd: "/w",
  project: "/w",
  startedAt: 0,
  status: { kind: "working", reason: "hook", since: 0 },
  lastEventAt: 0,
  history: [],
  ...over,
});

describe("tablero", () => {
  it("modelo, contexto, coste y git; sin datos, sin líneas", () => {
    const lines = boardLines(agent({ model: "Opus 5.5", contextPct: 62, costCents: 123 }), {
      branch: "main",
      ahead: 2,
      behind: 1,
      changed: 3,
    });
    expect(lines).toEqual([
      "Opus 5.5",
      "contexto 62 %",
      "$1.23 sesión",
      "main · 3 cambios · ↑2 · ↓1",
    ]);
    expect(boardLines(agent(), null)).toEqual([]);
    expect(boardLines(agent(), { ahead: 0, behind: 0, changed: 0 })).toEqual(["HEAD separado"]);
  });

  it("la última hora se reparte en tramos hasta ahora", () => {
    const H = 3_600_000;
    const now = 10 * H;
    const segs = timeline(
      [
        { kind: "idle", since: 0 }, // empieza antes de la ventana: se recorta
        { kind: "working", since: now - H / 2 },
        { kind: "waiting", since: now - H / 4 },
      ],
      now,
    );
    expect(segs).toEqual([
      { kind: "idle", from: 0, to: 0.5 },
      { kind: "working", from: 0.5, to: 0.75 },
      { kind: "waiting", from: 0.75, to: 1 },
    ]);
    expect(timeline([], now)).toEqual([]);
  });
});
