import { describe, expect, it } from "vitest";
import type { AgentState, ConversationInfo } from "../protocol";
import { dormantId, dormantRooms, isDormantId, roomEntries } from "./rooms";

const agent = (id: string, sessionId?: string): AgentState => ({
  id,
  provider: "claude",
  pid: 1,
  cwd: "/w/kiti/ui",
  project: "/w/kiti",
  startedAt: 0,
  status: { kind: "idle", reason: "hook", since: 0 },
  lastEventAt: 0,
  history: [],
  ...(sessionId === undefined ? {} : { sessionId }),
});

const conv = (sessionId: string, project = "/w/tienda"): ConversationInfo => ({
  sessionId,
  provider: "claude",
  cwd: project,
  project,
  updatedAt: 0,
});

describe("dormantRooms", () => {
  it("descarta las conversaciones que ya tienen un agente encendido", () => {
    const agents = new Map([["a", agent("a", "s1")]]);
    const rooms = dormantRooms(agents, [conv("s1"), conv("s2")], new Map());
    expect([...rooms.keys()]).toEqual([dormantId("s2")]);
    expect(isDormantId(dormantId("s2"))).toBe(true);
    expect(isDormantId("claude:1:2")).toBe(false);
  });

  it("devuelve el mismo mapa si nada cambió", () => {
    const list = [conv("s1")];
    const first = dormantRooms(new Map(), list, new Map());
    expect(dormantRooms(new Map(), list, first)).toBe(first);
    expect(dormantRooms(new Map(), [conv("s1")], first)).not.toBe(first);
  });
});

describe("roomEntries", () => {
  it("agrupa por proyecto: encendidas primero y después apagadas", () => {
    const agents = new Map([["a", agent("a")]]);
    const dormant = dormantRooms(agents, [conv("s2")], new Map());
    expect(roomEntries(agents, dormant)).toEqual([
      { id: "a", project: "/w/kiti" },
      { id: dormantId("s2"), project: "/w/tienda" },
    ]);
  });
});
