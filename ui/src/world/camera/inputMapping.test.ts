import { describe, expect, it } from "vitest";
import { gestureFactor, keyAction, wheelAction } from "./inputMapping";

const wheel = (
  deltaX: number,
  deltaY: number,
  mods: { ctrlKey?: boolean; metaKey?: boolean } = {},
) => ({
  deltaX,
  deltaY,
  deltaMode: 0,
  ctrlKey: mods.ctrlKey ?? false,
  metaKey: mods.metaKey ?? false,
});

describe("wheelAction", () => {
  it("rueda o dos dedos en el trackpad = pan", () => {
    expect(wheelAction(wheel(10, -20))).toEqual({ kind: "pan", dx: -10, dy: 20 });
  });

  it("ctrl + rueda = zoom (así llega el pinch en Chromium y WebView2)", () => {
    const zoomIn = wheelAction(wheel(0, -100, { ctrlKey: true }));
    const zoomOut = wheelAction(wheel(0, 100, { ctrlKey: true }));
    expect(zoomIn.kind).toBe("zoom");
    expect(zoomIn.kind === "zoom" && zoomIn.factor).toBeGreaterThan(1);
    expect(zoomOut.kind === "zoom" && zoomOut.factor).toBeLessThan(1);
  });

  it("cmd + rueda también es zoom", () => {
    expect(wheelAction(wheel(0, 50, { metaKey: true })).kind).toBe("zoom");
  });

  it("deltaMode en líneas se convierte a px", () => {
    expect(wheelAction({ ...wheel(0, 3), deltaMode: 1 })).toEqual({ kind: "pan", dx: -0, dy: -48 });
  });
});

describe("gestureFactor (pinch en WebKit / WKWebView)", () => {
  it("es el cociente entre escalas consecutivas", () => {
    expect(gestureFactor(1.0, 1.5)).toBeCloseTo(1.5);
    expect(gestureFactor(2, 1)).toBeCloseTo(0.5);
  });

  it("ignora escalas no válidas", () => {
    expect(gestureFactor(0, 1)).toBe(1);
  });
});

describe("keyAction", () => {
  it("0 ve todo, Esc vuelve a la vista general y +/- hacen zoom", () => {
    expect(keyAction("0")).toEqual({ kind: "fitAll" });
    expect(keyAction("Escape")).toEqual({ kind: "fitAll" });
    expect(keyAction("+")).toEqual({ kind: "zoomStep", factor: 1.25 });
    expect(keyAction("=")).toEqual({ kind: "zoomStep", factor: 1.25 });
    expect(keyAction("-")).toEqual({ kind: "zoomStep", factor: 0.8 });
    expect(keyAction("a")).toBeUndefined();
  });
});
