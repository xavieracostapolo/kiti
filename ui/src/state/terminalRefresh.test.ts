import { describe, expect, it } from "vitest";
import { decideTerminalRefresh, TERMINAL_REFRESH_GAP_MS } from "./terminalRefresh";

const base = { agentPids: [], knownPids: new Set<number>(), asked: new Set<number>(), lastAt: 0 };

describe("decideTerminalRefresh", () => {
  it("sin agentes nuevos sin terminal no pide nada", () => {
    expect(decideTerminalRefresh({ ...base, now: 10_000 })).toEqual({ refresh: false, mark: [] });
    expect(
      decideTerminalRefresh({ ...base, agentPids: [1], knownPids: new Set([1]), now: 10_000 }),
    ).toEqual({ refresh: false, mark: [] });
  });

  it("un agente sin terminal conocida pide la lista y se marca", () => {
    expect(
      decideTerminalRefresh({ ...base, agentPids: [1, 2], knownPids: new Set([1]), now: 10_000 }),
    ).toEqual({ refresh: true, mark: [2] });
  });

  it("no repite para un pid ya pedido (un agente de Ghostty nunca tendrá terminal)", () => {
    expect(
      decideTerminalRefresh({ ...base, agentPids: [2], asked: new Set([2]), now: 10_000 }),
    ).toEqual({ refresh: false, mark: [] });
  });

  it("como mucho una petición cada 5 s: espera lo que falta sin marcar", () => {
    const r = decideTerminalRefresh({ ...base, agentPids: [2], lastAt: 8_000, now: 10_000 });
    expect(r).toEqual({ refresh: false, mark: [], waitMs: TERMINAL_REFRESH_GAP_MS - 2_000 });
  });

  it("pasado el plazo, una sola petición marca todos los pids nuevos", () => {
    const r = decideTerminalRefresh({ ...base, agentPids: [2, 3], lastAt: 5_000, now: 10_000 });
    expect(r).toEqual({ refresh: true, mark: [2, 3] });
  });
});
