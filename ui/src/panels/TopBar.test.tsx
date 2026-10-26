// @vitest-environment jsdom
import { act, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { createKitiStore } from "../state/store";
import { TopBar } from "./TopBar";

describe("TopBar", () => {
  it("muestra SIMULADO cuando los datos vienen del simulador", () => {
    const store = createKitiStore();
    store.getState().setSource("sim");
    render(<TopBar store={store} />);
    expect(screen.getByText("SIMULADO")).toBeTruthy();
  });

  it("no muestra la etiqueta con datos reales", () => {
    const store = createKitiStore();
    store.getState().setSource("live");
    render(<TopBar store={store} />);
    expect(screen.queryByText("SIMULADO")).toBeNull();
  });

  it("con terminales ofrece lanzar un agente y elegir el CLI", async () => {
    const { default: userEvent } = await import("@testing-library/user-event");
    const store = createKitiStore();
    const launched: string[] = [];
    render(<TopBar store={store} onLaunch={(p) => launched.push(p)} />);
    await userEvent.click(screen.getByRole("button", { name: /nuevo agente/i }));
    // Gemini CLI se retiró para cuentas gratuitas: se ofrece su sucesor (ADR 0014).
    expect(screen.queryByRole("menuitem", { name: "Gemini CLI" })).toBeNull();
    await userEvent.click(await screen.findByRole("menuitem", { name: "Antigravity CLI" }));
    expect(launched).toEqual(["antigravity"]);
  });

  it("sin terminales (simulador) no hay botón de lanzar", () => {
    render(<TopBar store={createKitiStore()} />);
    expect(screen.queryByRole("button", { name: /nuevo agente/i })).toBeNull();
  });

  it("los contadores filtran por estado y se pueden combinar", async () => {
    const { default: userEvent } = await import("@testing-library/user-event");
    const store = createKitiStore();
    render(<TopBar store={store} />);
    const waiting = screen.getByTestId("count-waiting");
    await userEvent.click(waiting);
    expect(waiting.getAttribute("aria-pressed")).toBe("true");
    await userEvent.click(screen.getByTestId("count-error"));
    expect([...store.getState().filter.kinds]).toEqual(["waiting", "error"]);
    await userEvent.click(screen.getByRole("button", { name: "Quitar filtro" }));
    expect(store.getState().filter.kinds.size).toBe(0);
    expect(screen.queryByRole("button", { name: "Quitar filtro" })).toBeNull();
  });

  it("las apagadas tienen su contador solo si hay alguna", () => {
    const store = createKitiStore();
    render(<TopBar store={store} />);
    expect(screen.queryByTestId("count-dormant")).toBeNull();
    act(() =>
      store
        .getState()
        .setConversations([
          { sessionId: "s1", provider: "claude", cwd: "/w", project: "/w", updatedAt: 0 },
        ]),
    );
    expect(screen.getByTestId("count-dormant").textContent).toContain("1 apagadas");
  });

  it("/ enfoca el buscador y Esc limpia el filtro", async () => {
    const { default: userEvent } = await import("@testing-library/user-event");
    const store = createKitiStore();
    render(<TopBar store={store} />);
    await userEvent.keyboard("/");
    const search = screen.getByRole("searchbox", { name: "Buscar oficinas" });
    expect(document.activeElement).toBe(search);
    await userEvent.keyboard("tienda");
    expect(store.getState().filter.text).toBe("tienda");
    await userEvent.keyboard("{Escape}");
    expect(store.getState().filter.text).toBe("");
    expect(document.activeElement).not.toBe(search);
  });
});
