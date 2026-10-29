import { describe, expect, it } from "vitest";
import { applyMailCommand, fakeMail } from "./fakeMail";
import { SimFleet } from "./fleet";

const agents = new SimFleet({ count: 20, seed: 42 }).snapshot();
const start = () => fakeMail(agents, 1_000_000);

describe("fakeMail", () => {
  it("un hilo de cada caso, con alias por proveedor y un hilo pausado", () => {
    const mail = start();
    expect(mail.mode).toBe("auto");
    expect(mail.threads.map((t) => t.subject).sort()).toEqual([
      "Consulta",
      "Debate",
      "Reparto",
      "Revisión",
    ]);
    expect(mail.attention).toBe(1);
    expect(mail.aliases).toHaveLength(agents.length);
    const claude = mail.aliases.filter((a) => a.alias.startsWith("claude-"));
    expect(claude[0]?.alias).toBe("claude-1");
  });

  it("continuar suelta lo retenido y cerrar descarta lo pendiente", () => {
    let mail = start();
    const debate = mail.threads.find((t) => t.subject === "Debate");
    if (!debate) throw new Error("sin debate");
    mail = applyMailCommand(mail, { kind: "continue", id: debate.id });
    const after = mail.threads.find((t) => t.id === debate.id);
    expect(after?.state).toBe("open");
    expect(after?.messages.some((m) => m.state === "queued")).toBe(true);
    expect(mail.attention).toBe(0);
    mail = applyMailCommand(mail, { kind: "close", id: debate.id });
    expect(mail.threads.find((t) => t.id === debate.id)?.messages.at(-1)?.state).toBe("discarded");
  });

  it("modo manual, aprobar y pausar", () => {
    let mail = applyMailCommand(start(), { kind: "mode", mode: "manual" });
    expect(mail.mode).toBe("manual");
    mail = applyMailCommand(mail, { kind: "pause", paused: true });
    expect(mail.paused).toBe(true);
  });
});
