// @vitest-environment jsdom
import { act, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { createKitiStore } from "../state/store";
import { fakeQuota } from "../transport/sim/fakeQuota";
import { QuotaBadge } from "./QuotaBadge";

describe("QuotaBadge", () => {
  it("sin cuota no muestra nada", () => {
    const { container } = render(<QuotaBadge store={createKitiStore()} now={() => 0} />);
    expect(container.textContent).toBe("");
  });

  it("un chip por proveedor con lo peor de 5 h y de la semana", () => {
    const store = createKitiStore();
    render(<QuotaBadge store={store} now={() => 0} />);
    act(() => {
      for (const msg of fakeQuota(0)) store.getState().receive(msg);
    });
    expect(
      screen.getByRole("button", { name: "Cuota de Claude: 5 h 72 %, sem 40 %" }),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Cuota de Antigravity: 5 h 90 %, sem 18 %" }),
    ).toBeTruthy();
  });

  it("al pasar el ratón muestra cada ventana y cuándo se reinicia", async () => {
    const { default: userEvent } = await import("@testing-library/user-event");
    const store = createKitiStore();
    render(<QuotaBadge store={store} now={() => 0} />);
    act(() => {
      for (const msg of fakeQuota(0)) store.getState().receive(msg);
    });
    await userEvent.hover(screen.getByRole("button", { name: /Cuota de Antigravity/ }));
    const detail = await screen.findByRole("list", { name: "Detalle de la cuota de Antigravity" });
    expect(detail.textContent).toContain(
      "Claude and GPT models · semana: queda 18 % · se reinicia en 4 d 2 h",
    );
  });
});
