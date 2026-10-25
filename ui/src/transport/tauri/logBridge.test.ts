// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { installLogBridge } from "./logBridge";

describe("installLogBridge", () => {
  const originals = { info: console.info, warn: console.warn, error: console.error };
  afterEach(() => Object.assign(console, originals));

  it("reenvía console.warn/error al core y conserva la consola original", () => {
    const send = vi.fn();
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    installLogBridge(send);
    console.error("falló", new Error("boom"));
    expect(spy).toHaveBeenCalled();
    expect(send).toHaveBeenCalledWith("error", expect.stringContaining("falló boom"));
  });

  it("reenvía las promesas rechazadas sin capturar", () => {
    const send = vi.fn();
    installLogBridge(send);
    const event = new Event("unhandledrejection") as Event & { reason: unknown };
    event.reason = new Error("sin catch");
    window.dispatchEvent(event);
    expect(send).toHaveBeenCalledWith("error", expect.stringContaining("sin catch"));
  });
});
