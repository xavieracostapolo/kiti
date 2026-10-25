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
});
