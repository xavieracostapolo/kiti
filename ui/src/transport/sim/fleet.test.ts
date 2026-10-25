import { describe, expect, it } from "vitest";
import { projectFor, SimFleet } from "./fleet";

const ids = (f: SimFleet) => f.snapshot().map((a) => a.id);

describe("SimFleet", () => {
  it("crea N agentes con ids únicos", () => {
    const f = new SimFleet({ count: 20, seed: 42 });
    expect(new Set(ids(f)).size).toBe(20);
  });

  it("la misma semilla da el mismo mundo", () => {
    const a = new SimFleet({ count: 20, seed: 42 });
    const b = new SimFleet({ count: 20, seed: 42 });
    for (let i = 0; i < 200; i++) {
      expect(a.step()).toEqual(b.step());
    }
    expect(a.snapshot()).toEqual(b.snapshot());
  });

  it("otra semilla da otro mundo", () => {
    const a = new SimFleet({ count: 20, seed: 1 });
    const b = new SimFleet({ count: 20, seed: 2 });
    for (let i = 0; i < 100; i++) {
      a.step();
      b.step();
    }
    expect(a.snapshot()).not.toEqual(b.snapshot());
  });

  it("cada agente es independiente: añadir agentes no altera a los primeros", () => {
    const small = new SimFleet({ count: 5, seed: 7 });
    const big = new SimFleet({ count: 20, seed: 7 });
    for (let i = 0; i < 150; i++) {
      small.step();
      big.step();
    }
    expect(big.snapshot().slice(0, 5)).toEqual(small.snapshot());
  });

  it("el tiempo virtual avanza 100 ms por paso y no usa el reloj real", () => {
    const f = new SimFleet({ count: 3, seed: 1 });
    const t0 = f.now;
    f.step();
    f.step();
    expect(f.now - t0).toBe(200);
  });

  it("step solo devuelve los agentes que cambiaron", () => {
    const f = new SimFleet({ count: 20, seed: 42 });
    const before = new Map(f.snapshot().map((a) => [a.id, a]));
    const { upserts } = f.step();
    for (const a of upserts) expect(a).not.toEqual(before.get(a.id));
  });

  it("kill: el agente desaparece en el siguiente paso (salida esperada, no dead)", () => {
    const f = new SimFleet({ count: 4, seed: 3 });
    const target = ids(f)[1] as string;
    expect(f.kill(target)).toBe(true);
    expect(f.step().removed).toEqual([target]);
    expect(ids(f)).not.toContain(target);
    expect(f.kill(target)).toBe(false);
  });

  it("con muchos pasos aparecen los cinco colores", () => {
    const f = new SimFleet({ count: 20, seed: 42 });
    const seen = new Set<string>();
    for (let i = 0; i < 600; i++) {
      f.step();
      for (const a of f.snapshot()) seen.add(a.status.kind);
    }
    expect([...seen].sort()).toEqual(["dead", "error", "idle", "waiting", "working"]);
  });

  it("varias oficinas comparten proyecto, sin depender de cuántos agentes haya", () => {
    const projects = (n: number) =>
      new SimFleet({ count: n, seed: 1 }).snapshot().map((a) => a.project);
    const big = projects(20);
    expect(new Set(big).size).toBeLessThan(big.length);
    expect(projects(5)).toEqual(big.slice(0, 5));
    expect(projectFor(0)).toBe(projectFor(2));
    expect(projectFor(2)).not.toBe(projectFor(3));
  });

  it("solo Claude trae título, actividad y último pedido", () => {
    const f = new SimFleet({ count: 20, seed: 42 });
    const seen = { activity: false, prompt: false };
    for (let i = 0; i < 400; i++) {
      for (const a of f.step().upserts) {
        if (a.provider !== "claude") {
          expect(a.title ?? a.activity ?? a.lastPrompt).toBeUndefined();
          continue;
        }
        expect(a.title).toBeDefined();
        if (a.activity) seen.activity = true;
        if (a.lastPrompt) seen.prompt = true;
        if (a.status.kind === "idle") expect(a.activity).toBeUndefined();
      }
    }
    expect(seen).toEqual({ activity: true, prompt: true });
  });
});
