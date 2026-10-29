// @vitest-environment jsdom
import { act, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { AgentState } from "../protocol";
import { dormantId } from "../state/rooms";
import { createKitiStore } from "../state/store";
import { OfficeTooltip } from "./OfficeTooltip";

const HOUR = 3_600_000;

const agent: AgentState = {
  id: "claude:1:2",
  provider: "claude",
  pid: 1,
  cwd: "/Users/dev/billing",
  project: "/Users/dev/billing",
  startedAt: 0,
  status: { kind: "waiting", reason: "hook", since: 0 },
  lastEventAt: 0,
  history: [],
};

function setup() {
  const store = createKitiStore();
  store.getState().receive({ type: "Snapshot", seq: 0, agents: [agent] });
  render(<OfficeTooltip store={store} now={() => 3 * HOUR} />);
  return store;
}

describe("OfficeTooltip", () => {
  it("muestra nombre, proveedor y estado de la oficina bajo el ratón", () => {
    const store = setup();
    expect(screen.queryByRole("tooltip")).toBeNull();
    act(() => store.getState().setHover({ id: agent.id, x: 100, y: 50 }));
    const tip = screen.getByRole("tooltip");
    expect(tip.textContent).toContain("billing");
    expect(tip.textContent).toContain("claude");
    expect(tip.textContent).toContain("esperando");
    act(() => store.getState().setHover(undefined));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("dice en qué trabaja sin abrir la terminal: título, actividad y último pedido", () => {
    const store = setup();
    const busy = {
      ...agent,
      title: "Revisar vulnerabilidades",
      activity: "Editando package.json",
      lastPrompt: "parchea los CVE",
    };
    act(() => {
      store.getState().receive({ type: "Diff", seq: 1, ingestMs: 0, upserts: [busy], removed: [] });
      store.getState().setHover({ id: agent.id, x: 1, y: 1 });
    });
    const tip = screen.getByRole("tooltip").textContent;
    expect(tip).toContain("Revisar vulnerabilidades");
    expect(tip).toContain("Editando package.json");
    expect(tip).toContain("parchea los CVE");
  });

  it("una oficina apagada dice cuándo se cerró y que se puede retomar", () => {
    const store = setup();
    act(() => {
      store.getState().setConversations([
        {
          sessionId: "s1",
          provider: "claude",
          cwd: "/Users/dev/tienda-web",
          project: "/Users/dev/tienda-web",
          title: "Migración de datos",
          updatedAt: HOUR,
        },
      ]);
      store.getState().setHover({ id: dormantId("s1"), x: 1, y: 1 });
    });
    const tip = screen.getByRole("tooltip").textContent;
    expect(tip).toContain("Migración de datos");
    expect(tip).toContain("tienda-web · cerrada hace 2 h");
    expect(tip).toContain("retomarla");
    // Al seleccionarla, su panel ya lo dice: el tooltip se quita.
    act(() => store.getState().select(dormantId("s1")));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});
