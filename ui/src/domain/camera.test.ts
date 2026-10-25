import fc from "fast-check";
import { describe, expect, it } from "vitest";
import {
  type Camera,
  clampToBounds,
  fitRect,
  type Limits,
  panBy,
  toScreen,
  toWorld,
  tween,
  type Viewport,
  zoomAt,
} from "./camera";

const vp: Viewport = { w: 1440, h: 900 };
const limits: Limits = { kMin: 0.25, kMax: 6 };
const cam: Camera = { x: 500, y: 300, k: 2 };

const close = (a: number, b: number) => Math.abs(a - b) < 1e-6;

describe("conversión de coordenadas", () => {
  it("el centro de la cámara cae en el centro del viewport", () => {
    expect(toScreen(cam, vp, { x: 500, y: 300 })).toEqual({ x: 720, y: 450 });
  });

  it("toWorld es la inversa de toScreen", () => {
    const p = { x: 123, y: 456 };
    const back = toWorld(cam, vp, toScreen(cam, vp, p));
    expect(close(back.x, p.x) && close(back.y, p.y)).toBe(true);
  });
});

describe("zoomAt", () => {
  it("propiedad: el punto bajo el cursor no se mueve", () => {
    fc.assert(
      fc.property(
        fc.double({ min: 0, max: 1440, noNaN: true }),
        fc.double({ min: 0, max: 900, noNaN: true }),
        fc.double({ min: 0.5, max: 2, noNaN: true }),
        (sx, sy, factor) => {
          const before = toWorld(cam, vp, { x: sx, y: sy });
          const next = zoomAt(cam, vp, { x: sx, y: sy }, factor, limits);
          const after = toWorld(next, vp, { x: sx, y: sy });
          expect(Math.abs(after.x - before.x)).toBeLessThan(1e-6);
          expect(Math.abs(after.y - before.y)).toBeLessThan(1e-6);
        },
      ),
    );
  });

  it("respeta kMin y kMax", () => {
    expect(zoomAt(cam, vp, { x: 0, y: 0 }, 100, limits).k).toBe(6);
    expect(zoomAt(cam, vp, { x: 0, y: 0 }, 0.001, limits).k).toBe(0.25);
  });
});

describe("panBy", () => {
  it("arrastrar a la derecha mueve la cámara a la izquierda, en px de pantalla", () => {
    expect(panBy(cam, 100, -50)).toEqual({ x: 450, y: 325, k: 2 });
  });
});

describe("fitRect", () => {
  it("encuadra el rectángulo completo con margen y lo centra", () => {
    const r = { x: 0, y: 0, w: 1000, h: 500 };
    const c = fitRect(r, vp, 20, limits);
    expect(c.x).toBe(500);
    expect(c.y).toBe(250);
    expect(c.k).toBeCloseTo(Math.min(1440 / 1040, 900 / 540));
  });

  it("no pasa de kMax con un rectángulo diminuto", () => {
    expect(fitRect({ x: 0, y: 0, w: 1, h: 1 }, vp, 0, limits).k).toBe(6);
  });
});

describe("clampToBounds", () => {
  it("el centro de la cámara no sale del mundo", () => {
    const bounds = { x: 0, y: 0, w: 1000, h: 600 };
    expect(clampToBounds({ x: -50, y: 900, k: 1 }, bounds)).toEqual({ x: 0, y: 600, k: 1 });
    expect(clampToBounds(cam, bounds)).toEqual(cam);
  });
});

describe("tween", () => {
  const to: Camera = { x: 100, y: 50, k: 4 };

  it("empieza en el origen, termina en el destino y marca done", () => {
    const t = tween(cam, to, 1000, 400);
    expect(t.at(1000)).toEqual({ camera: cam, done: false });
    expect(t.at(1400)).toEqual({ camera: to, done: true });
    expect(t.at(5000)).toEqual({ camera: to, done: true });
  });

  it("en la mitad está entre los dos extremos", () => {
    const mid = tween(cam, to, 0, 400).at(200).camera;
    expect(mid.k).toBeGreaterThan(2);
    expect(mid.k).toBeLessThan(4);
    expect(mid.x).toBeLessThan(500);
    expect(mid.x).toBeGreaterThan(100);
  });

  it("con duración 0 salta directo al destino", () => {
    expect(tween(cam, to, 0, 0).at(0)).toEqual({ camera: to, done: true });
  });
});
