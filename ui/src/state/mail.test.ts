import { describe, expect, it } from "vitest";
import type { MailMessage, MailState } from "../protocol";
import { aliasOf, chipOf, inTransit, waitingFor } from "./mail";

const msg = (id: number, from: string, to: string, state: MailMessage["state"]): MailMessage => ({
  id,
  from,
  fromAlias: `${from.split(":")[0]}-1`,
  to,
  toAlias: `${to.split(":")[0]}-1`,
  text: `m${id}`,
  sentAt: id,
  state,
});

const mail = (messages: MailMessage[], extra: Partial<MailState> = {}): MailState => ({
  mode: "auto",
  paused: false,
  attention: 0,
  aliases: [
    { agent: "claude:1:1", alias: "claude-1" },
    { agent: "codex:2:1", alias: "codex-1" },
  ],
  threads: [{ id: 1, subject: "s", hops: messages.length, limit: 12, state: "open", messages }],
  ...extra,
});

describe("mail", () => {
  it("alias de cada oficina", () => {
    const m = mail([]);
    expect(aliasOf(m, "codex:2:1")).toBe("codex-1");
    expect(aliasOf(m, "agy:9:1")).toBeUndefined();
    expect(aliasOf(undefined, "codex:2:1")).toBeUndefined();
  });

  it("cuenta lo que espera a cada destinatario (sin lo entregado ni lo descartado)", () => {
    const m = mail([
      msg(1, "claude:1:1", "codex:2:1", "queued"),
      msg(2, "claude:1:1", "codex:2:1", "pending"),
      msg(3, "claude:1:1", "codex:2:1", "held"),
      msg(4, "claude:1:1", "codex:2:1", "delivered"),
      msg(5, "codex:2:1", "claude:1:1", "discarded"),
    ]);
    expect(waitingFor(m, "codex:2:1")).toBe(3);
    expect(waitingFor(m, "claude:1:1")).toBe(0);
  });

  it("un sobre por pareja con mensajes en camino", () => {
    const m = mail([
      msg(1, "claude:1:1", "codex:2:1", "queued"),
      msg(2, "claude:1:1", "codex:2:1", "queued"),
      msg(3, "codex:2:1", "claude:1:1", "delivered"),
    ]);
    expect(inTransit(m)).toEqual([{ from: "claude:1:1", to: "codex:2:1", count: 2 }]);
    expect(inTransit(undefined)).toEqual([]);
  });

  it("el chip pide atención solo con algo por decidir", () => {
    expect(chipOf(undefined)).toBeUndefined();
    expect(chipOf(mail([msg(1, "claude:1:1", "codex:2:1", "queued")]))).toEqual({
      count: 1,
      attention: false,
    });
    expect(chipOf(mail([], { attention: 2 }))).toEqual({ count: 2, attention: true });
    expect(chipOf(mail([]))).toEqual({ count: 0, attention: false });
  });
});
