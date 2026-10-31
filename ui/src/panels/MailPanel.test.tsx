// @vitest-environment jsdom
import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { MailCommand, MailState } from "../protocol";
import { createKitiStore } from "../state/store";
import { MailChip, MailPanel } from "./MailPanel";

const MAIL: MailState = {
  mode: "auto",
  paused: false,
  attention: 1,
  aliases: [
    { agent: "claude:1:1", alias: "claude-1" },
    { agent: "codex:2:1", alias: "codex-1" },
  ],
  threads: [
    {
      id: 7,
      subject: "Debate",
      hops: 13,
      limit: 12,
      state: "paused",
      messages: [
        {
          id: 3,
          from: "codex:2:1",
          fromAlias: "codex-1",
          to: "claude:1:1",
          toAlias: "claude-1",
          text: "La caché va en el servidor.",
          sentAt: 0,
          state: "held",
        },
      ],
    },
  ],
};

function setup(mail: MailState = MAIL) {
  const store = createKitiStore();
  const acts: MailCommand[] = [];
  const actFn = vi.fn((c: MailCommand) => acts.push(c));
  render(
    <>
      <MailChip store={store} />
      <MailPanel store={store} now={() => 65_000} act={actFn} />
    </>,
  );
  act(() => store.getState().receive({ type: "Mail", ...mail }));
  return { store, acts };
}

describe("MailPanel", () => {
  it("sin estado del core no hay chip ni panel", () => {
    const store = createKitiStore();
    const { container } = render(<MailChip store={store} />);
    expect(container.textContent).toBe("");
  });

  it("el chip marca aria-pressed y lo pulsa de nuevo para cerrar", () => {
    setup();
    const chip = screen.getByRole("button", { name: "Mensajes: 1 por decidir" });
    expect(chip.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(chip);
    expect(chip.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(chip);
    expect(chip.getAttribute("aria-pressed")).toBe("false");
    expect(screen.queryByRole("complementary", { name: "Mensajes entre agentes" })).toBeNull();
  });

  it("el chip pide atención y abre el panel con el hilo pausado", () => {
    const { acts } = setup();
    const chip = screen.getByRole("button", { name: "Mensajes: 1 por decidir" });
    fireEvent.click(chip);
    const thread = screen.getByRole("region", { name: "Hilo Debate" });
    expect(thread.textContent).toContain("codex-1 ⇄ claude-1");
    expect(thread.textContent).toContain("pausado 12/12");
    expect(thread.textContent).toContain("hace 1 min · retenido");
    fireEvent.click(screen.getByRole("button", { name: "Continuar (+12)" }));
    fireEvent.click(screen.getByRole("button", { name: "Cerrar hilo" }));
    expect(acts).toEqual([
      { kind: "continue", id: 7 },
      { kind: "close", id: 7 },
    ]);
  });

  it("automático por defecto; cambiar a manual y pausar mandan la acción", () => {
    const { acts } = setup();
    fireEvent.click(screen.getByRole("button", { name: /Mensajes/ }));
    expect(screen.getByRole("button", { name: "Automático" }).getAttribute("aria-pressed")).toBe(
      "true",
    );
    fireEvent.click(screen.getByRole("button", { name: "Manual" }));
    fireEvent.click(screen.getByRole("button", { name: "⏸ Pausar" }));
    expect(acts).toEqual([
      { kind: "mode", mode: "manual" },
      { kind: "pause", paused: true },
    ]);
  });

  it("en manual, un mensaje por aprobar se entrega o se descarta", () => {
    const thread = MAIL.threads[0];
    const message = thread?.messages[0];
    if (!thread || !message) throw new Error("fixture");
    const { acts, store } = setup({
      ...MAIL,
      mode: "manual",
      threads: [
        { ...thread, state: "open", hops: 1, messages: [{ ...message, state: "pending" }] },
      ],
    });
    act(() => store.getState().openSidePanel("mail"));
    fireEvent.click(screen.getByRole("button", { name: "Entregar" }));
    fireEvent.click(screen.getByRole("button", { name: "Descartar" }));
    expect(acts).toEqual([
      { kind: "approve", id: 3 },
      { kind: "discard", id: 3 },
    ]);
  });
});
