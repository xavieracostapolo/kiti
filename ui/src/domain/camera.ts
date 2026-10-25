// Cámara propia mínima (ADR 0003 §5, ADR 0006 §3). Pura: el tiempo se inyecta.
//
// `x`, `y` es el punto del mundo que queda en el centro del viewport; `k` es la escala
// (px de pantalla por px de mundo). pantalla = (mundo - centro) · k + viewport / 2.

import type { Rect } from "./layout";

export type Camera = { x: number; y: number; k: number };
export type Viewport = { w: number; h: number };
export type Limits = { kMin: number; kMax: number };
export type Point = { x: number; y: number };

export const DEFAULT_LIMITS: Limits = { kMin: 0.25, kMax: 6 };
export const FOCUS_MS = 400;

const clampK = (k: number, limits: Limits) => Math.min(limits.kMax, Math.max(limits.kMin, k));

export function toScreen(cam: Camera, vp: Viewport, p: Point): Point {
  return { x: (p.x - cam.x) * cam.k + vp.w / 2, y: (p.y - cam.y) * cam.k + vp.h / 2 };
}

export function toWorld(cam: Camera, vp: Viewport, s: Point): Point {
  return { x: (s.x - vp.w / 2) / cam.k + cam.x, y: (s.y - vp.h / 2) / cam.k + cam.y };
}

/** Zoom en torno a un punto de pantalla: el punto del mundo bajo el cursor no se mueve. */
export function zoomAt(
  cam: Camera,
  vp: Viewport,
  screen: Point,
  factor: number,
  limits: Limits,
): Camera {
  const k = clampK(cam.k * factor, limits);
  const anchor = toWorld(cam, vp, screen);
  return {
    x: anchor.x - (screen.x - vp.w / 2) / k,
    y: anchor.y - (screen.y - vp.h / 2) / k,
    k,
  };
}

/** Desplaza la cámara siguiendo un arrastre de `dx`, `dy` px de pantalla. */
export function panBy(cam: Camera, dx: number, dy: number): Camera {
  return { x: cam.x - dx / cam.k, y: cam.y - dy / cam.k, k: cam.k };
}

/** Encuadra `rect` completo con `padding` px de pantalla a cada lado. */
export function fitRect(rect: Rect, vp: Viewport, padding: number, limits: Limits): Camera {
  const k = clampK(Math.min(vp.w / (rect.w + 2 * padding), vp.h / (rect.h + 2 * padding)), limits);
  return { x: rect.x + rect.w / 2, y: rect.y + rect.h / 2, k };
}

/** Márgenes de pantalla ocupados por paneles: la barra superior y la terminal a la derecha. */
export type Insets = { top: number; right: number };

/** Como `fitRect`, pero dentro de la zona libre del viewport, centrado en ella. */
export function fitInto(
  rect: Rect,
  vp: Viewport,
  insets: Insets,
  padding: number,
  limits: Limits,
): Camera {
  const free = { w: Math.max(1, vp.w - insets.right), h: Math.max(1, vp.h - insets.top) };
  const c = fitRect(rect, free, padding, limits);
  // El centro de la zona libre está desplazado respecto al del viewport: se compensa en mundo.
  return { x: c.x + insets.right / 2 / c.k, y: c.y - insets.top / 2 / c.k, k: c.k };
}

/** El centro de la cámara no sale del mundo, así que el mundo nunca desaparece de la vista. */
export function clampToBounds(cam: Camera, bounds: Rect): Camera {
  return {
    x: Math.min(bounds.x + bounds.w, Math.max(bounds.x, cam.x)),
    y: Math.min(bounds.y + bounds.h, Math.max(bounds.y, cam.y)),
    k: cam.k,
  };
}

const easeInOutCubic = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2);

export type Tween = { at(nowMs: number): { camera: Camera; done: boolean } };

/** Interpola posición en lineal y escala en espacio logarítmico, con easing. */
export function tween(from: Camera, to: Camera, startMs: number, durationMs: number): Tween {
  return {
    at(nowMs) {
      const t = durationMs <= 0 ? 1 : Math.min(1, Math.max(0, (nowMs - startMs) / durationMs));
      if (t >= 1) return { camera: to, done: true };
      if (t <= 0) return { camera: from, done: false };
      const e = easeInOutCubic(t);
      return {
        camera: {
          x: from.x + (to.x - from.x) * e,
          y: from.y + (to.y - from.y) * e,
          k: Math.exp(Math.log(from.k) + (Math.log(to.k) - Math.log(from.k)) * e),
        },
        done: false,
      };
    },
  };
}
