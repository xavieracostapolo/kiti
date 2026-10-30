import { describe, expect, it } from "vitest";
import type { ServerMsg } from "../../protocol";
import { SimTransport } from "./SimTransport";

function manualTimer() {
  let tick: (() => void) | undefined;
  return {
    schedule: (fn: () => void) => {
      tick = fn;
      return () => {
        tick = undefined;
      };
    },
    fire: (n = 1) => {
      for (let i = 0; i < n; i++) tick?.();
    },
    get active() {
      return tick !== undefined;
    },
  };
}

describe("SimTransport", () => {
  it("el primer mensaje es Snapshot y Hello es de tipo sim", async () => {
    const msgs: ServerMsg[] = [];
    const t = new SimTransport({ count: 5, seed: 1, schedule: manualTimer().schedule });
    const hello = await t.connect((m) => msgs.push(m));
    expect(hello.source).toBe("sim");
    expect(msgs[0]?.type).toBe("Snapshot");
    expect(msgs[0]?.type === "Snapshot" && msgs[0].agents.length).toBe(5);
    expect(t.killConfirmation).toBe("in-app");
  });

  it("con tasks emite un mensaje Tasks y sin él no", async () => {
    const withTasks: ServerMsg[] = [];
    await new SimTransport({
      count: 5,
      seed: 1,
      tasks: true,
      schedule: manualTimer().schedule,
    }).connect((m) => withTasks.push(m));
    const tasks = withTasks.find((m) => m.type === "Tasks");
    expect(tasks?.type === "Tasks" && tasks.tasks.length).toBeGreaterThan(0);
    const without: ServerMsg[] = [];
    await new SimTransport({ count: 5, seed: 1, schedule: manualTimer().schedule }).connect((m) =>
      without.push(m),
    );
    expect(without.some((m) => m.type === "Tasks")).toBe(false);
  });

  it("los Diff llevan seq monotónico sin huecos", async () => {
    const timer = manualTimer();
    const msgs: ServerMsg[] = [];
    const t = new SimTransport({ count: 20, seed: 42, schedule: timer.schedule });
    await t.connect((m) => msgs.push(m));
    timer.fire(100);
    // Solo Snapshot y Diff llevan seq; Git y Quota van aparte (ADR 0015, 0016).
    const seqs = msgs.flatMap((m) => ("seq" in m ? [m.seq] : []));
    expect(seqs).toEqual(seqs.map((_, i) => i));
  });

  it("steps avanza antes del Snapshot y freeze no programa pasos", async () => {
    const timer = manualTimer();
    const frozen = new SimTransport({
      count: 3,
      seed: 1,
      steps: 50,
      freeze: true,
      schedule: timer.schedule,
    });
    await frozen.connect(() => {});
    expect(timer.active).toBe(false);
  });

  it("killAgent con freeze emite el Diff de borrado al momento", async () => {
    const msgs: ServerMsg[] = [];
    const t = new SimTransport({
      count: 3,
      seed: 1,
      freeze: true,
      schedule: manualTimer().schedule,
    });
    await t.connect((m) => msgs.push(m));
    const first = msgs[0];
    const id = first?.type === "Snapshot" ? first.agents[0]?.id : undefined;
    expect(id).toBeDefined();
    expect(await t.killAgent(id as string)).toEqual({ kind: "killed" });
    const last = msgs.at(-1);
    expect(last?.type === "Diff" && last.removed).toEqual([id]);
    expect(await t.killAgent("no-existe")).toEqual({ kind: "notFound" });
  });

  it("now() expone el reloj virtual y avanza con los pasos", async () => {
    const t = new SimTransport({ count: 2, seed: 1, schedule: manualTimer().schedule });
    const t0 = t.now();
    t.advance(3);
    expect(t.now() - t0).toBe(300);
  });

  it("dispose detiene la simulación", async () => {
    const timer = manualTimer();
    const t = new SimTransport({ count: 3, seed: 1, schedule: timer.schedule });
    await t.connect(() => {});
    t.dispose();
    expect(timer.active).toBe(false);
  });
});
