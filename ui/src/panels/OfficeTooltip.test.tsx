// @vitest-environment jsdom
import { act, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { AgentState } from "../protocol";
import { createKitiStore } from "../state/store";
import { OfficeTooltip } from "./OfficeTooltip";

const agent: AgentState = {
  id: "claude:1:2",
  provider: "claude",
  pid: 1,
  cwd: "/Users/dev/billing",
  startedAt: 0,
  status: { kind: "waiting", reason: "hook", since: 0 },
  lastEventAt: 0,
};

describe("OfficeTooltip", () => {
  it("muestra nombre, proveedor y estado de la oficina bajo el ratón", () => {
    const store = createKitiStore();
    store.getState().receive({ type: "Snapshot", seq: 0, agents: [agent] });
    render(<OfficeTooltip store={store} />);
    expect(screen.queryByRole("tooltip")).toBeNull();
    act(() => store.getState().setHover({ id: agent.id, x: 100, y: 50 }));
    const tip = screen.getByRole("tooltip");
    expect(tip.textContent).toContain("billing");
    expect(tip.textContent).toContain("claude");
    expect(tip.textContent).toContain("esperando");
    act(() => store.getState().setHover(undefined));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});
