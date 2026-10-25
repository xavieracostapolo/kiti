// Mundo Pixi imperativo (ADR 0003 §3 y §5): se suscribe al store fuera de React, aplica solo las
// operaciones de planScene y pinta bajo demanda con RenderScheduler.

import "pixi.js/unsafe-eval";
import {
  Application,
  BitmapFont,
  Container,
  Culler,
  Graphics,
  type Texture,
  TextureStyle,
  Ticker,
  TilingSprite,
} from "pixi.js";
import type { Zoom } from "../dev/params";
import {
  type Camera,
  clampToBounds,
  DEFAULT_LIMITS,
  FOCUS_MS,
  fitRect,
  panBy,
  toScreen,
  toWorld,
  tween,
  type Viewport,
  zoomAt,
} from "../domain/camera";
import {
  assignSlots,
  DEFAULT_LAYOUT,
  type LayoutOpts,
  type Rect,
  slotRect,
  worldBounds,
} from "../domain/layout";
import { LOD_SCALE, type Lod, lodFor } from "../domain/lod";
import type { AgentId, AgentState } from "../protocol";
import { planScene } from "../state/agents";
import type { KitiStore } from "../state/store";
import type { Metrics } from "../telemetry/metrics";
import { attachCameraInput } from "./camera/CameraInput";
import { FONT, OfficeView } from "./OfficeView";
import { drawOfficeTemplate } from "./officeTemplate";
import { PAL } from "./palette";
import { RenderScheduler } from "./RenderScheduler";

export type WorldOptions = {
  metrics: Metrics;
  anim: boolean;
  zoom: Zoom;
  focus?: string;
  layout?: LayoutOpts;
};

export type BenchResult = {
  frames: number;
  p50: number;
  p95: number;
  p99: number;
  frameMsP95: number;
};

export type WorldHandle = {
  /** Se resuelve tras el primer frame con la cámara inicial aplicada. */
  ready: Promise<void>;
  focus(idOrIndex: string): void;
  fitAll(): void;
  setZoom(zoom: Zoom): void;
  bench(durationMs?: number): Promise<BenchResult>;
  frames(): number;
  /** Centro de la oficina en px de pantalla (tests E2E). */
  screenCenter(idOrIndex: string): { x: number; y: number } | undefined;
  /** Escala actual de la cámara. */
  scale(): number;
  dispose(): void;
};

const FIT_PADDING = 32;
/** Alto que ocupa la barra superior: el encuadre general no queda debajo de ella. */
const TOP_INSET = 56;
const FOCUS_PADDING = 24;
const EMPTY = new Map<AgentId, AgentState>();

export function createWorld(el: HTMLElement, store: KitiStore, opts: WorldOptions): WorldHandle {
  const layout = opts.layout ?? DEFAULT_LAYOUT;
  const room = { w: layout.roomW * layout.tile, h: layout.roomH * layout.tile, tile: layout.tile };
  const views = new Map<AgentId, OfficeView>();
  const cleanup: (() => void)[] = [];
  let slots = new Map<AgentId, number>();
  let bounds: Rect = worldBounds(slots, layout);
  let vp: Viewport = { w: el.clientWidth || 1, h: el.clientHeight || 1 };
  let cam: Camera = { x: bounds.w / 2, y: bounds.h / 2, k: 1 };
  let lod: Lod = "far";
  let animToken = 0;
  let pendingDiffAt: number | undefined;
  let scheduler: RenderScheduler | undefined;
  let disposed = false;
  let benchSamples: { deltas: number[]; frameMs: number[] } | undefined;

  const worldRoot = new Container();
  const officesLayer = new Container();
  let corridor: TilingSprite | undefined;
  let template: Texture | undefined;

  const ready = (async () => {
    TextureStyle.defaultOptions.scaleMode = "nearest";
    const app = new Application();
    await app.init({
      width: vp.w,
      height: vp.h,
      autoStart: false,
      antialias: false,
      roundPixels: true,
      background: PAL.background,
      resolution: window.devicePixelRatio || 1,
      autoDensity: true,
      preference: "webgl",
    });
    if (disposed) {
      app.destroy(true, { children: true });
      return;
    }
    cleanup.push(() => app.destroy(true, { children: true, texture: true }));
    // El EventSystem de Pixi cuelga un listener de Ticker.system que pide un rAF a 60 Hz para
    // siempre (hover de sus eventos). No usamos eventos de Pixi (la cámara escucha el DOM), y
    // ese bucle gastaba CPU en reposo en el WebView y en el proceso de la app. Hay que quitar
    // también `autoStart`: si no, el siguiente `Ticker.system.add()` de Pixi lo vuelve a arrancar.
    Ticker.system.autoStart = false;
    Ticker.system.stop();
    el.appendChild(app.canvas);

    BitmapFont.install({
      name: FONT,
      style: {
        fontFamily: "Menlo, Monaco, monospace",
        fontSize: 36,
        fill: PAL.text,
        fontWeight: "bold",
      },
      chars: [["a", "z"], ["A", "Z"], ["0", "9"], " -_.:!/()[]#·…?"],
      resolution: 1,
      textureStyle: { scaleMode: "linear" },
    });

    template = app.renderer.generateTexture({ target: drawOfficeTemplate(room), resolution: 1 });
    const tile = new Graphics()
      .rect(0, 0, layout.tile, layout.tile)
      .fill(PAL.corridor)
      .rect(layout.tile / 2 - 1, layout.tile / 2 - 1, 2, 2)
      .fill(PAL.corridorDot);
    corridor = new TilingSprite({
      texture: app.renderer.generateTexture(tile),
      width: 1,
      height: 1,
    });

    worldRoot.addChild(corridor, officesLayer);
    app.stage.addChild(worldRoot);

    let firstFrame: (() => void) | undefined;
    const firstPaint = new Promise<void>((resolve) => {
      firstFrame = resolve;
    });

    scheduler = new RenderScheduler({
      render: () => {
        Culler.shared.cull(app.stage, app.renderer.screen);
        app.renderer.render(app.stage);
        if (pendingDiffAt !== undefined) {
          opts.metrics.diffToPaint(performance.now() - pendingDiffAt);
          pendingDiffAt = undefined;
        }
      },
      onFrame: (ms, delta) => {
        opts.metrics.frame(ms, delta);
        if (benchSamples) {
          benchSamples.frameMs.push(ms);
          if (delta !== undefined) benchSamples.deltas.push(delta);
        }
        let visible = 0;
        for (const v of views.values()) if (!v.root.culled) visible++;
        opts.metrics.setGauges({
          officesVisible: visible,
          lod,
          tickerRunning: scheduler?.running ?? false,
        });
        firstFrame?.();
        firstFrame = undefined;
      },
    });
    cleanup.push(() => scheduler?.dispose());

    sync(EMPTY, store.getState().agents.agents);
    applyInitialCamera();

    cleanup.push(
      store.subscribe((s, prev) => {
        if (s.agents.agents !== prev.agents.agents) {
          sync(prev.agents.agents, s.agents.agents);
          pendingDiffAt ??= performance.now();
          scheduler?.request();
        }
        if (s.view.selectedId !== prev.view.selectedId) {
          if (prev.view.selectedId) views.get(prev.view.selectedId)?.setSelected(false);
          if (s.view.selectedId) views.get(s.view.selectedId)?.setSelected(true);
          scheduler?.request();
        }
      }),
    );

    cleanup.push(
      attachCameraInput(app.canvas, {
        pan: (dx, dy) => setCamera(panBy(cam, dx, dy)),
        zoomAt: (sx, sy, f) => setCamera(zoomAt(cam, vp, { x: sx, y: sy }, f, DEFAULT_LIMITS)),
        zoomCenter: (f) =>
          setCamera(zoomAt(cam, vp, { x: vp.w / 2, y: vp.h / 2 }, f, DEFAULT_LIMITS)),
        fitAll: () => setCamera(fitAllCamera(), true),
        click: (sx, sy) => store.getState().select(officeAt(sx, sy)),
        focusAt: (sx, sy) => {
          const id = officeAt(sx, sy);
          if (id) focusId(id);
        },
        dragging: () => {},
        hover: (p) => {
          const id = p ? officeAt(p.x, p.y) : undefined;
          store.getState().setHover(id && p ? { id, x: p.x, y: p.y } : undefined);
        },
      }),
    );

    const observer = new ResizeObserver(() => {
      vp = { w: el.clientWidth || 1, h: el.clientHeight || 1 };
      app.renderer.resize(vp.w, vp.h);
      applyCamera();
    });
    observer.observe(el);
    cleanup.push(() => observer.disconnect());

    const onVisibility = () => (document.hidden ? scheduler?.pause() : scheduler?.resume());
    document.addEventListener("visibilitychange", onVisibility);
    cleanup.push(() => document.removeEventListener("visibilitychange", onVisibility));

    scheduler.request();
    await firstPaint;
  })();

  function sync(prev: ReadonlyMap<AgentId, AgentState>, next: ReadonlyMap<AgentId, AgentState>) {
    if (!template) return;
    const ops = planScene(prev, next);
    if (ops.length === 0) return;
    slots = assignSlots(slots, [...next.keys()]);
    for (const op of ops) {
      if (op.op === "remove") {
        views.get(op.id)?.root.destroy({ children: true });
        views.delete(op.id);
        continue;
      }
      let view = views.get(op.agent.id);
      if (!view) {
        view = new OfficeView(template, room);
        view.setLod(lod);
        view.setSelected(store.getState().view.selectedId === op.agent.id);
        views.set(op.agent.id, view);
        officesLayer.addChild(view.root);
      }
      const r = slotRect(slots.get(op.agent.id) as number, layout);
      view.root.position.set(r.x, r.y);
      view.update(op.agent);
    }
    bounds = worldBounds(slots, layout);
    if (corridor) {
      const margin = layout.tile * 40;
      corridor.position.set(-margin, -margin);
      corridor.width = bounds.w + margin * 2;
      corridor.height = bounds.h + margin * 2;
    }
  }

  function applyCamera(request = true) {
    cam = clampToBounds(cam, bounds);
    worldRoot.scale.set(cam.k);
    worldRoot.position.set(vp.w / 2 - cam.x * cam.k, vp.h / 2 - cam.y * cam.k);
    const nextLod = lodFor(cam.k, lod);
    if (nextLod !== lod) {
      lod = nextLod;
      for (const v of views.values()) v.setLod(lod);
      store.getState().setLod(lod);
    }
    if (request) scheduler?.request();
  }

  function setCamera(target: Camera, animate = false) {
    const token = ++animToken;
    if (!animate || !opts.anim || !scheduler) {
      cam = target;
      applyCamera();
      return;
    }
    const tw = tween(cam, target, performance.now(), FOCUS_MS);
    scheduler.animate((now) => {
      if (token !== animToken) return true;
      const r = tw.at(now);
      cam = r.camera;
      applyCamera(false);
      return r.done;
    });
  }

  function fitAllCamera(): Camera {
    const c = fitRect(bounds, { w: vp.w, h: vp.h - TOP_INSET }, FIT_PADDING, DEFAULT_LIMITS);
    return { ...c, y: c.y - TOP_INSET / 2 / c.k };
  }

  function officeAt(sx: number, sy: number): AgentId | undefined {
    const p = toWorld(cam, vp, { x: sx, y: sy });
    for (const [id, slot] of slots) {
      const r = slotRect(slot, layout);
      if (p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h) return id;
    }
    return undefined;
  }

  function resolveId(idOrIndex: string): AgentId | undefined {
    if (slots.has(idOrIndex)) return idOrIndex;
    if (/^\d+$/.test(idOrIndex))
      return [...store.getState().agents.agents.keys()][Number(idOrIndex)];
    return undefined;
  }

  function focusId(id: AgentId) {
    const slot = slots.get(id);
    if (slot === undefined) return;
    setCamera(fitRect(slotRect(slot, layout), vp, FOCUS_PADDING, DEFAULT_LIMITS), true);
  }

  /** Oficina más cercana al centro del mundo, para los zooms mid y near. */
  function centralRect(): Rect {
    const cx = bounds.w / 2;
    const cy = bounds.h / 2;
    let best: Rect = { x: cx, y: cy, w: 0, h: 0 };
    let bestD = Number.POSITIVE_INFINITY;
    for (const slot of slots.values()) {
      const r = slotRect(slot, layout);
      const d = Math.hypot(r.x + r.w / 2 - cx, r.y + r.h / 2 - cy);
      if (d < bestD) {
        bestD = d;
        best = r;
      }
    }
    return best;
  }

  function zoomCamera(zoom: Zoom): Camera {
    if (zoom === "far") return fitAllCamera();
    const r = centralRect();
    return { x: r.x + r.w / 2, y: r.y + r.h / 2, k: LOD_SCALE[zoom] };
  }

  function applyInitialCamera() {
    const id = opts.focus ? resolveId(opts.focus) : undefined;
    const slot = id === undefined ? undefined : slots.get(id);
    cam =
      slot === undefined
        ? zoomCamera(opts.zoom)
        : fitRect(slotRect(slot, layout), vp, FOCUS_PADDING, DEFAULT_LIMITS);
    if (id !== undefined) {
      store.getState().select(id);
      views.get(id)?.setSelected(true); // aún no hay suscripción al store
    }
    applyCamera(false);
  }

  return {
    ready,
    focus(idOrIndex) {
      const id = resolveId(idOrIndex);
      if (id) focusId(id);
    },
    fitAll: () => setCamera(fitAllCamera(), true),
    setZoom: (zoom) => setCamera(zoomCamera(zoom), true),
    frames: () => scheduler?.frames ?? 0,
    screenCenter(idOrIndex) {
      const id = resolveId(idOrIndex);
      const slot = id === undefined ? undefined : slots.get(id);
      if (slot === undefined) return undefined;
      const r = slotRect(slot, layout);
      return toScreen(cam, vp, { x: r.x + r.w / 2, y: r.y + r.h / 2 });
    },
    scale: () => cam.k,
    async bench(durationMs = 10_000) {
      await ready;
      const s = scheduler;
      if (!s) return { frames: 0, p50: 0, p95: 0, p99: 0, frameMsP95: 0 };
      benchSamples = { deltas: [], frameMs: [] };
      const start = performance.now();
      const base = fitAllCamera();
      await new Promise<void>((resolve) => {
        s.animate((now) => {
          const t = (now - start) / durationMs;
          // Recorrido guionizado: zoom de ida y vuelta mientras se hace pan en círculo.
          const k = base.k * (1 + 1.5 * Math.sin(Math.PI * Math.min(1, t)));
          cam = {
            x: bounds.w / 2 + Math.cos(t * Math.PI * 4) * bounds.w * 0.3,
            y: bounds.h / 2 + Math.sin(t * Math.PI * 4) * bounds.h * 0.3,
            k,
          };
          applyCamera(false);
          if (t >= 1) {
            resolve();
            return true;
          }
          return false;
        });
      });
      const samples = benchSamples;
      benchSamples = undefined;
      const pct = (arr: number[], p: number) => {
        if (arr.length === 0) return 0;
        const sorted = [...arr].sort((a, b) => a - b);
        return sorted[Math.max(0, Math.ceil((p / 100) * sorted.length) - 1)] as number;
      };
      return {
        frames: samples.frameMs.length,
        p50: pct(samples.deltas, 50),
        p95: pct(samples.deltas, 95),
        p99: pct(samples.deltas, 99),
        frameMsP95: pct(samples.frameMs, 95),
      };
    },
    dispose() {
      disposed = true;
      for (const fn of cleanup.splice(0).reverse()) fn();
      views.clear();
    },
  };
}
