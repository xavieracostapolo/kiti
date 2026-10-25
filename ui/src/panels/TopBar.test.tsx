// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
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
    await userEvent.click(await screen.findByRole("menuitem", { name: "Claude Code" }));
    expect(launched).toEqual(["claude"]);
  });

  it("sin terminales (simulador) no hay botón de lanzar", () => {
    render(<TopBar store={createKitiStore()} />);
    expect(screen.queryByRole("button", { name: /nuevo agente/i })).toBeNull();
  });
});
