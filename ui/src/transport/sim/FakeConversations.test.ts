import { describe, expect, it } from "vitest";
import { FakeConversations, fakeConversations } from "./FakeConversations";
import { SimTransport } from "./SimTransport";

describe("FakeConversations", () => {
  it("da cuatro conversaciones cerradas, dos en un proyecto sin agentes", () => {
    const list = fakeConversations("/Users/dev/kiti", 10 * 3_600_000);
    expect(list).toHaveLength(4);
    expect(list.filter((c) => c.project === "/Users/dev/thoth-assets")).toHaveLength(2);
    expect(list.every((c) => c.updatedAt < 10 * 3_600_000)).toBe(true);
  });

  it("retomar una la quita de la lista; una desconocida da error", async () => {
    const list = fakeConversations("/w/kiti", 0);
    const resumed = { id: "sim-r", pid: 7, provider: "claude" as const, cwd: "/w", running: true };
    const fake = new FakeConversations(list, resumed);
    const id = list[0]?.sessionId ?? "";
    await expect(fake.resume(id)).resolves.toBe(resumed);
    expect((await fake.list()).map((c) => c.sessionId)).not.toContain(id);
    await expect(fake.resume(id)).rejects.toMatchObject({ code: "CONVERSATION_NOT_FOUND" });
  });

  it("el simulador solo las ofrece con ?conversations=1", () => {
    expect(new SimTransport({ count: 3, seed: 1 }).conversations).toBeUndefined();
    expect(
      new SimTransport({ count: 3, seed: 1, conversations: true }).conversations,
    ).toBeDefined();
  });
});
