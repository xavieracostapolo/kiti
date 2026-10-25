import { describe, expect, it } from "vitest";
import { FakeTerminals } from "./FakeTerminals";
import { SimTransport } from "./SimTransport";

const text = (chunks: Uint8Array[]) => chunks.map((c) => new TextDecoder().decode(c)).join("");

describe("FakeTerminals", () => {
  it("se une a un agente, saluda al conectar y repite lo que se escribe", async () => {
    const t = new FakeTerminals({ pid: 42, provider: "codex", cwd: "/w/x" });
    expect(await t.launch(), "sin agente lanzable").toBeNull();
    expect(await t.list()).toEqual([
      { id: "sim-1", pid: 42, provider: "codex", cwd: "/w/x", running: true },
    ]);
    const out: Uint8Array[] = [];
    await t.attach("sim-1", (b) => out.push(b));
    await t.input("sim-1", "hola");
    await t.input("sim-1", "\r");
    await t.resize();
    expect(text(out)).toBe("Terminal simulada de Kiti\r\n$ hola\r\n$ ");
  });

  it("SimTransport solo la crea con terminals=1 y la une al primer agente", async () => {
    expect(new SimTransport({ count: 3, seed: 1 }).terminals).toBeUndefined();
    const sim = new SimTransport({ count: 3, seed: 1, terminals: true });
    const [info] = (await sim.terminals?.list()) ?? [];
    let first: number | undefined;
    await sim.connect((m) => {
      if (m.type === "Snapshot") first = m.agents[0]?.pid;
    });
    expect(info?.pid).toBe(first);
  });

  it("Nuevo agente une una segunda terminal al agente lanzable, una sola vez", async () => {
    const t = new FakeTerminals(
      { pid: 1, provider: "claude", cwd: "/a" },
      { pid: 2, provider: "gemini", cwd: "/b" },
    );
    expect(await t.launch()).toEqual({
      id: "sim-2",
      pid: 2,
      provider: "gemini",
      cwd: "/b",
      running: true,
    });
    expect((await t.list()).map((x) => x.pid)).toEqual([1, 2]);
    expect(await t.launch()).toBeNull();
  });
});
