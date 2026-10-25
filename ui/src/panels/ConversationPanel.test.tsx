// @vitest-environment jsdom
import { act, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ConversationInfo } from "../protocol";
import { dormantId } from "../state/rooms";
import { createKitiStore } from "../state/store";
import { ConversationPanel } from "./ConversationPanel";

const conv: ConversationInfo = {
  sessionId: "s1",
  provider: "claude",
  cwd: "/Users/dev/thoth-assets/api",
  project: "/Users/dev/thoth-assets",
  title: "Revisar vulnerabilidades",
  lastPrompt: "parchea los CVE",
  updatedAt: 0,
};

function setup(onResume?: (id: string) => Promise<void>) {
  const store = createKitiStore();
  store.getState().setConversations([conv]);
  store.getState().select(dormantId("s1"));
  render(
    <ConversationPanel
      store={store}
      world={() => undefined}
      now={() => 120_000}
      {...(onResume ? { onResume } : {})}
    />,
  );
  return store;
}

describe("ConversationPanel", () => {
  it("muestra la conversación cerrada y la retoma", async () => {
    const { default: userEvent } = await import("@testing-library/user-event");
    const onResume = vi.fn(async () => {});
    setup(onResume);
    expect(screen.getByText("Revisar vulnerabilidades")).toBeTruthy();
    expect(screen.getByText("thoth-assets")).toBeTruthy();
    expect(screen.getByText("parchea los CVE")).toBeTruthy();
    expect(screen.getByText("hace 2 min")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Retomar" }));
    expect(onResume).toHaveBeenCalledWith("s1");
  });

  it("si no se puede retomar, lo explica", async () => {
    const { default: userEvent } = await import("@testing-library/user-event");
    setup(async () => {
      throw { code: "FOLDER", message: "La carpeta ya no existe" };
    });
    await userEvent.click(screen.getByRole("button", { name: "Retomar" }));
    expect((await screen.findByRole("alert")).textContent).toBe("La carpeta ya no existe");
  });

  it("sin la app nativa no se ofrece retomar; con la terminal abierta se oculta", () => {
    const store = setup();
    expect(screen.queryByRole("button", { name: "Retomar" })).toBeNull();
    act(() => store.getState().openTerminal("kiti-1"));
    expect(screen.queryByLabelText("Conversación cerrada")).toBeNull();
  });
});
