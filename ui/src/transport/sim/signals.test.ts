import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { type AgentStatus, applySignal, type Signal } from "./signals";

type Step = {
  signal: Signal;
  expect: "removed" | { kind: string; reason: string; lastTool?: string };
};
type Case = { name: string; steps: Step[]; then?: Step[] };

const dir = join(import.meta.dirname, "../../../../fixtures/sim/conformance");
const cases = readdirSync(dir)
  .filter((f) => f.endsWith(".json"))
  .map((f) => JSON.parse(readFileSync(join(dir, f), "utf8")) as Case);

function run(steps: Step[]) {
  let status: AgentStatus | undefined;
  steps.forEach((step, i) => {
    const next = applySignal(status, step.signal, 1000 + i);
    if (step.expect === "removed") {
      expect(next, `paso ${i}`).toBe("removed");
      return;
    }
    expect(next, `paso ${i}`).not.toBe("removed");
    const s = next as AgentStatus;
    expect({ kind: s.kind, reason: s.reason, lastTool: s.lastTool }, `paso ${i}`).toEqual({
      lastTool: undefined,
      ...step.expect,
    });
    status = s;
  });
}

describe("conformidad del mapeo señal → semáforo (fixtures/sim/conformance)", () => {
  it("hay fixtures", () => expect(cases.length).toBeGreaterThan(0));

  for (const c of cases) {
    it(c.name, () => {
      run(c.steps);
      if (c.then) run(c.then);
    });
  }

  it("since solo cambia cuando cambia el color", () => {
    const a = applySignal(undefined, { type: "PromptSubmitted" }, 10) as AgentStatus;
    const b = applySignal(a, { type: "ToolStarted", tool: "Read" }, 20) as AgentStatus;
    expect(b.since).toBe(10);
    const c = applySignal(b, { type: "AwaitingInput" }, 30) as AgentStatus;
    expect(c.since).toBe(30);
  });
});
