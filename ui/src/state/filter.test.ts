import { describe, expect, it } from "vitest";
import type { AgentState, ConversationInfo } from "../protocol";
import { EMPTY_FILTER, isActive, matchesAgent, matchesConversation, toggleKind } from "./filter";

const agent = (over: Partial<AgentState> = {}): AgentState => ({
  id: "a",
  provider: "claude",
  pid: 1,
  cwd: "/w/tienda-web/api",
  project: "/w/tienda-web",
  startedAt: 0,
  status: { kind: "waiting", reason: "hook", since: 0 },
  lastEventAt: 0,
  history: [],
  title: "Revisar vulnerabilidades",
  ...over,
});

const conv: ConversationInfo = {
  sessionId: "s1",
  provider: "claude",
  cwd: "/w/kiti",
  project: "/w/kiti",
  title: "Migración de datos",
  updatedAt: 0,
};

describe("filtro de oficinas", () => {
  it("vacío: todo coincide y no está activo", () => {
    expect(isActive(EMPTY_FILTER)).toBe(false);
    expect(matchesAgent(agent(), EMPTY_FILTER)).toBe(true);
    expect(matchesConversation(conv, EMPTY_FILTER)).toBe(true);
  });

  it("por estado, con varios a la vez; las apagadas son su propio estado", () => {
    const waiting = toggleKind(EMPTY_FILTER, "waiting");
    expect(isActive(waiting)).toBe(true);
    expect(matchesAgent(agent(), waiting)).toBe(true);
    expect(
      matchesAgent(agent({ status: { kind: "idle", reason: "hook", since: 0 } }), waiting),
    ).toBe(false);
    expect(matchesConversation(conv, waiting)).toBe(false);
    const both = toggleKind(waiting, "dormant");
    expect(matchesConversation(conv, both)).toBe(true);
    // Pulsar otra vez lo quita.
    expect(isActive(toggleKind(waiting, "waiting"))).toBe(false);
  });

  it("el texto busca en título, proyecto y carpeta, sin mayúsculas ni tildes", () => {
    const f = (text: string) => ({ ...EMPTY_FILTER, text });
    expect(matchesAgent(agent(), f("VULNERA"))).toBe(true);
    expect(matchesAgent(agent(), f("tienda"))).toBe(true);
    expect(matchesAgent(agent(), f("api"))).toBe(true);
    expect(matchesAgent(agent(), f("billing"))).toBe(false);
    expect(matchesConversation(conv, f("migracion"))).toBe(true);
    expect(matchesConversation(conv, f("  "))).toBe(true);
  });

  it("estado y texto se combinan", () => {
    const f = { ...toggleKind(EMPTY_FILTER, "waiting"), text: "kiti" };
    expect(matchesAgent(agent(), f)).toBe(false);
  });
});
