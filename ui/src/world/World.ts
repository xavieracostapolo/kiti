// Mundo Pixi imperativo (ADR 0003 §3 y §5): se suscribe al store fuera de React, aplica solo las
// operaciones de planScene y pinta bajo demanda con RenderScheduler.

import "pixi.js/unsafe-eval";
import {
  Application,
  BitmapFont,
  BitmapText,
  Container,
  Culler,
  Graphics,
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
  fitInto,
  fitRect,
  panBy,
  toScreen,
  toWorld,
  tween,
  type Viewport,
  zoomAt,
} from "../domain/camera";
import { DEFAULT_LAYOUT, type LayoutOpts, type Rect, union } from "../domain/layout";
import { LOD_SCALE, type Lod, lodFor } from "../domain/lod";
import { fontFor } from "../domain/text";
import { DEFAULT_ZONES, layoutZones, type Zone, type ZoneSlots } from "../domain/zones";
import type { AgentId, AgentState, ConversationInfo, Provider } from "../protocol";
import { planScene } from "../state/agents";
import { isActive, matchesAgent, matchesConversation } from "../state/filter";
import {
  aliasOf,
  hasLingering,
  MAIL_LINGER_MS,
  markDelivered,
  visibleTransit,
  waitingFor,
} from "../state/mail";
import { isDormantId, roomEntries } from "../state/rooms";
import type { KitiStore } from "../state/store";
import type { Metrics } from "../telemetry/metrics";
import { attachCameraInput } from "./camera/CameraInput";
import {
  loadSheets,
  PEOPLE_ANIM_FRAMES,
  PEOPLE_BACK_COLUMN,
  PEOPLE_ROWS,
  tileTexture,
} from "./kenney";
import { FONT, type OfficeArt, OfficeView } from "./OfficeView";
import { DECOR_VARIANTS, drawOfficeTemplate } from "./officeTemplate";
import { PAL } from "./palette";
import { RenderScheduler } from "./RenderScheduler";

export type WorldOptions = {
  metrics: Metrics;
  anim: boolean;
  zoom: Zoom;
  focus?: string;
  layout?: LayoutOpts;
  /** Reloj para la línea de tiempo del tablero (el del simulador en modo sim). */
  now?: () => number;
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
  zoomStep(factor: number): void;
  bench(durationMs?: number): Promise<BenchResult>;
  frames(): number;
  /** Centro de la oficina en px de pantalla (tests E2E). */
  screenCenter(idOrIndex: string): { x: number; y: number } | undefined;
  /** Escala actual de la cámara. */
  scale(): number;
  /** Zonas por proyecto con su número de oficinas (tests E2E). */
  zones(): { name: string; count: number }[];
  /** Ids de las oficinas atenuadas por el filtro (tests E2E). */
  dimmed(): string[];
  /** Texto del tablero de una oficina si se ve (tests E2E, ADR 0016). */
  board(id: string): string | undefined;
  /** Alias y sobre de una oficina (tests E2E, ADR 0017). */
  mail(id: string): string | undefined;
  /** Sobres dibujados entre oficinas (tests E2E, ADR 0017). */
  envelopes(): number;
  dispose(): void;
};

const FIT_PADDING = 32;
/** Alto que ocupa la barra superior: el encuadre general no queda debajo de ella. */
const TOP_INSET = 56;
const FOCUS_PADDING = 24;
const EMPTY = new Map<AgentId, AgentState>();
const NO_ROOMS = new Map<string, ConversationInfo>();
/** Caracteres de la fuente bitmap: los títulos y pedidos llegan en español (ADR 0009). */
const FONT_CHARS = " -_.:!/()[]#·…?¿¡,;'\"<>«»=+*&%$@áéíóúüñÁÉÍÓÚÜÑ↑↓";

export function createWorld(el: HTMLElement, store: KitiStore, opts: WorldOptions): WorldHandle {
  const layout = opts.layout ?? DEFAULT_LAYOUT;
  const room = { w: layout.roomW * layout.tile, h: layout.roomH * layout.tile, tile: layout.tile };
  const views = new Map<AgentId, OfficeView>();
  const cleanup: (() => void)[] = [];
  /** Oficinas y zonas por proyecto (ADR 0009). */
  let zoneSlots: ZoneSlots = new Map();
  let rooms = new Map<string, Rect>();
  let zones: Zone[] = [];
  /** Rótulos de las zonas: su letra también se adapta al zoom. */
  let zoneLabels: { text: BitmapText; zone: Zone }[] = [];
  let zoneFont = 18;
  let bounds: Rect = layoutZones(zoneSlots, [], layout).bounds;
  let vp: Viewport = { w: el.clientWidth || 1, h: el.clientHeight || 1 };
  let cam: Camera = { x: bounds.w / 2, y: bounds.h / 2, k: 1 };
  let lod: Lod = "far";
  let animToken = 0;
  let pendingDiffAt: number | undefined;
  let scheduler: RenderScheduler | undefined;
  let disposed = false;
  let benchSamples: { deltas: number[]; frameMs: number[] } | undefined;

  const worldRoot = new Container();
  const zonesLayer = new Container();
  const officesLayer = new Container();
  /** Rutas y sobres de los mensajes en camino entre oficinas (ADR 0017). */
  const mailLayer = new Container();
  let envelopeCount = 0;
  /** Cuándo vio la UI pasar cada mensaje a entregado (id → ms de `performance.now()`). */
  const deliveredSeen = new Map<number, number>();
  let mailAnimating = false;
  let mailExpiry: ReturnType<typeof setTimeout> | undefined;
  let corridor: TilingSprite | undefined;
  let art: OfficeArt | undefined;

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
      chars: [["a", "z"], ["A", "Z"], ["0", "9"], FONT_CHARS],
      resolution: 1,
      textureStyle: { scaleMode: "linear" },
    });

    // Tiles de Kenney (ADR 0013): variantes de sala rasterizadas por proveedor, compartidas por sus oficinas.
    const sheets = await loadSheets();
    if (disposed) return;
    const templatesFor = (provider: Provider) =>
      Array.from({ length: DECOR_VARIANTS }, (_, variant) =>
        app.renderer.generateTexture({
          target: drawOfficeTemplate(room, sheets, provider, variant),
          resolution: 1,
        }),
      );
    art = {
      templates: {
        claude: templatesFor("claude"),
        codex: templatesFor("codex"),
        gemini: templatesFor("gemini"),
        antigravity: templatesFor("antigravity"),
      },
      people: PEOPLE_ROWS.map((row) =>
        PEOPLE_ANIM_FRAMES.map((df) => tileTexture(sheets, "urban", PEOPLE_BACK_COLUMN, row + df)),
      ),
    };
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

    worldRoot.addChild(corridor, zonesLayer, officesLayer, mailLayer);
    app.stage.addChild(worldRoot);

    let firstFrame: (() => void) | undefined;
    const firstPaint = new Promise<void>((resolve) => {
      firstFrame = resolve;
    });

    scheduler = new RenderScheduler({
      render: () => {
        // `false`: con las transformaciones de este frame. Con las del anterior (el valor por
        // defecto), tras un salto de cámara sin animación las oficinas quedaban ocultas.
        Culler.shared.cull(app.stage, app.renderer.screen, false);
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
    cleanup.push(() => clearTimeout(mailExpiry));

    sync(EMPTY, NO_ROOMS);
    applyInitialCamera();
    // Pudo pedirse antes de existir el mundo (clic en un aviso con la ventana cerrada).
    takePendingLaunch();

    cleanup.push(
      store.subscribe((s, prev) => {
        if (s.agents.agents !== prev.agents.agents || s.dormant !== prev.dormant) {
          sync(prev.agents.agents, prev.dormant);
          pendingDiffAt ??= performance.now();
          scheduler?.request();
        }
        takePendingLaunch();
        // Al abrir, cerrar o redimensionar la terminal cambia la zona libre: se recolocan las zonas
        // para la proporción libre y se encuadran todas las oficinas a la izquierda del panel.
        if (
          s.openTerminalId !== prev.openTerminalId ||
          s.terminalFraction !== prev.terminalFraction
        ) {
          relayout();
          setCamera(fitAllCamera(), s.openTerminalId !== prev.openTerminalId);
        }
        if (s.terminals !== prev.terminals || s.agents.agents !== prev.agents.agents) {
          for (const [id, view] of views) {
            const agent = s.agents.agents.get(id);
            view.setHasTerminal(agent !== undefined && s.terminals.has(agent.pid));
          }
          scheduler?.request();
        }
        if (
          s.filter !== prev.filter ||
          s.agents.agents !== prev.agents.agents ||
          s.dormant !== prev.dormant
        ) {
          applyFilter();
          scheduler?.request();
        }
        // Al cambiar el filtro, la cámara encuadra lo que coincide (o todo, si se quita).
        if (s.filter !== prev.filter) setCamera(filterCamera(), true);
        // Mensajes entre agentes (ADR 0017): alias, sobres por oficina y rutas.
        if (s.mail !== prev.mail) {
          markDelivered(s.mail, prev.mail, deliveredSeen, performance.now());
          applyMail();
          scheduler?.request();
          trackLingering();
        }
        // Git del proyecto de cada oficina (ADR 0016).
        if (s.git !== prev.git) {
          for (const [id, view] of views) {
            const agent = s.agents.agents.get(id);
            if (agent) view.setGit(s.git.get(agent.project));
          }
          scheduler?.request();
        }
        if (s.view.selectedId !== prev.view.selectedId) {
          if (prev.view.selectedId) views.get(prev.view.selectedId)?.setSelected(false);
          if (s.view.selectedId) views.get(s.view.selectedId)?.setSelected(true);
          scheduler?.request();
        }
        if (s.layoutMode !== prev.layoutMode) {
          relayout();
          setCamera(fitAllCamera(), true);
        }
        if (s.hover?.id !== prev.hover?.id) {
          if (prev.hover?.id) views.get(prev.hover.id)?.setHovered(false);
          if (s.hover?.id) views.get(s.hover.id)?.setHovered(true);
          app.canvas.style.cursor = s.hover?.id ? "pointer" : "default";
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
        click: (sx, sy) => clickOffice(officeAt(sx, sy)),
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

  /**
   * Agente recién lanzado desde Kiti, o el de un aviso en el que se hizo clic: en cuanto existe su
   * oficina, se enfoca y se selecciona.
   */
  function takePendingLaunch() {
    const s = store.getState();
    if (s.pendingLaunchPid === undefined) return;
    const agent = [...s.agents.agents.values()].find((a) => a.pid === s.pendingLaunchPid);
    if (!agent) return;
    s.setPendingLaunch(undefined);
    s.select(agent.id);
    focusId(agent.id);
  }

  function newView(id: string): OfficeView {
    const view = new OfficeView(art as OfficeArt, room, opts.now ?? Date.now);
    view.setLod(lod);
    view.setTextScale(cam.k);
    view.setSelected(store.getState().view.selectedId === id);
    views.set(id, view);
    officesLayer.addChild(view.root);
    return view;
  }

  function dropView(id: string) {
    views.get(id)?.root.destroy({ children: true });
    views.delete(id);
  }

  /** Aplica a la escena lo que cambió en agentes y conversaciones cerradas, y recoloca todo. */
  function sync(prev: ReadonlyMap<AgentId, AgentState>, prevDormant: ReadonlyMap<string, unknown>) {
    if (!art) return;
    const { agents: state, dormant, terminals } = store.getState();
    const next = state.agents;
    const ops = planScene(prev, next);
    if (ops.length === 0 && dormant === prevDormant) return;
    for (const op of ops) {
      if (op.op === "remove") {
        dropView(op.id);
        continue;
      }
      const view = views.get(op.agent.id) ?? newView(op.agent.id);
      view.update(op.agent);
      view.setGit(store.getState().git.get(op.agent.project));
      // Las terminales pueden haber llegado antes de que existiera la oficina.
      view.setHasTerminal(terminals.has(op.agent.pid));
    }
    if (dormant !== prevDormant) {
      for (const id of prevDormant.keys()) if (!dormant.has(id)) dropView(id);
      for (const [id, c] of dormant) (views.get(id) ?? newView(id)).showConversation(c);
    }
    relayout();
  }

  /** Recoloca las zonas según el modo de distribución y el espacio libre actual. */
  function relayout() {
    if (!art) return;
    const { agents: state, dormant, layoutMode } = store.getState();
    const l = layoutZones(zoneSlots, roomEntries(state.agents, dormant), layout, {
      ...DEFAULT_ZONES,
      mode: layoutMode,
      aspect: Math.max(1, vp.w - insets().right) / Math.max(1, vp.h - TOP_INSET),
    });
    zoneSlots = l.slots;
    rooms = l.rooms;
    bounds = l.bounds;
    for (const [id, r] of rooms) views.get(id)?.root.position.set(r.x, r.y);
    drawZones(l.zones);
    applyMail();
    if (corridor) {
      const margin = layout.tile * 40;
      corridor.position.set(-margin, -margin);
      corridor.width = bounds.w + margin * 2;
      corridor.height = bounds.h + margin * 2;
    }
  }

  /**
   * Un entregado sigue en el mapa `MAIL_LINGER_MS`, moviéndose de una oficina a la otra. El
   * ticker solo corre mientras haya alguno; al irse el último vuelve a 0 frames en reposo. Sin
   * animaciones (`anim=0`) no hay ticker: un temporizador redibuja al caducar.
   */
  function trackLingering() {
    clearTimeout(mailExpiry);
    const mail = store.getState().mail;
    if (!hasLingering(mail, performance.now(), deliveredSeen)) return;
    if (!opts.anim || !scheduler) {
      mailExpiry = setTimeout(() => {
        applyMail();
        scheduler?.request();
      }, MAIL_LINGER_MS + 50);
      return;
    }
    if (mailAnimating) return;
    mailAnimating = true;
    scheduler.animate((now) => {
      applyMail(now);
      if (hasLingering(store.getState().mail, now, deliveredSeen)) return false;
      mailAnimating = false;
      return true;
    });
  }

  /**
   * Alias y sobre de cada oficina, y una ruta punteada con un sobre entre cada pareja con
   * mensajes en camino (ADR 0017). Los que esperan son estáticos; los recién entregados se
   * mueven (ver `trackLingering`).
   */
  function applyMail(now = performance.now()) {
    const mail = store.getState().mail;
    for (const [id, view] of views) view.setMail(aliasOf(mail, id), waitingFor(mail, id));
    for (const child of mailLayer.removeChildren()) child.destroy({ children: true });
    envelopeCount = 0;
    const t = layout.tile;
    for (const tr of visibleTransit(mail, now, deliveredSeen)) {
      const a = rooms.get(tr.from);
      const b = rooms.get(tr.to);
      if (!a || !b) continue;
      // En el tercio superior (a la altura de la etiqueta del alias) para no tapar el cartel de título.
      const ax = a.x + a.w / 2;
      const ay = a.y + a.h * 0.25;
      const bx = b.x + b.w / 2;
      const by = b.y + b.h * 0.25;
      const len = Math.hypot(bx - ax, by - ay);
      if (len < 1) continue;
      const g = new Graphics();
      const dash = t * 0.6;
      const gap = t * 0.4;
      const ux = (bx - ax) / len;
      const uy = (by - ay) / len;
      for (let d = 0; d < len; d += dash + gap) {
        const e = Math.min(len, d + dash);
        g.moveTo(ax + ux * d, ay + uy * d).lineTo(ax + ux * e, ay + uy * e);
      }
      g.stroke({ color: PAL.mailPath, width: Math.max(2, t * 0.25), alpha: 0.9 });
      // Sobre en el punto medio, con su sombra y la solapa en V.
      const ew = t * 2.2;
      const eh = t * 1.5;
      const ex = ax + (bx - ax) * tr.progress - ew / 2;
      const ey = ay + (by - ay) * tr.progress - eh / 2;
      const line = Math.max(1.5, t * 0.18);
      g.rect(ex + line * 2, ey + line * 2, ew, eh)
        .fill(PAL.housing)
        .rect(ex, ey, ew, eh)
        .fill(PAL.envelope)
        .stroke({ color: PAL.mailInk, width: line })
        .moveTo(ex, ey)
        .lineTo(ex + ew / 2, ey + eh * 0.55)
        .lineTo(ex + ew, ey)
        .stroke({ color: PAL.mailInk, width: line })
        .rect(ex + ew - t * 0.35, ey - t * 0.35, t * 0.7, t * 0.7)
        .fill(PAL.mailFill)
        .stroke({ color: PAL.mailInk, width: line * 0.7 });
      mailLayer.addChild(g);
      if (tr.count > 1) {
        const n = new BitmapText({
          text: String(tr.count),
          style: { fontFamily: FONT, fontSize: t * 0.9 },
        });
        n.tint = PAL.mailInk;
        n.position.set(ex + ew / 2 - n.width / 2, ey + eh * 0.45);
        mailLayer.addChild(n);
      }
      envelopeCount++;
    }
  }

  /** Un área por proyecto con su nombre: pocas, se redibujan enteras. */
  function drawZones(next: Zone[]) {
    const same =
      next.length === zones.length &&
      next.every((z, i) => {
        const o = zones[i];
        return (
          o &&
          o.project === z.project &&
          o.count === z.count &&
          o.x === z.x &&
          o.y === z.y &&
          o.w === z.w &&
          o.h === z.h
        );
      });
    zones = next;
    if (same) return;
    for (const child of zonesLayer.removeChildren()) child.destroy({ children: true });
    zoneLabels = [];
    const t = layout.tile;
    for (const z of zones) {
      const g = new Graphics()
        // Sombra proyectada y canto frontal inferior para que la zona se vea como una plataforma 2.5D.
        .roundRect(z.x + 5, z.y + 8, z.w, z.h + 4, 4)
        .fill({ color: 0x000000, alpha: 0.36 })
        .rect(z.x, z.y + z.h, z.w, 6)
        .fill(0x1e2331)
        .rect(z.x, z.y, z.w, z.h)
        .fill(PAL.zoneFloor)
        .rect(z.x, z.y, z.w, 2 * t)
        .fill(PAL.zoneHeader)
        .rect(z.x, z.y + 2 * t, z.w, 3)
        .fill({ color: 0x000000, alpha: 0.22 })
        .rect(z.x, z.y, z.w, z.h)
        .stroke({ color: PAL.zoneBorder, width: 2 })
        .rect(z.x + 1, z.y + 1, z.w - 2, 1)
        .fill({ color: 0xffffff, alpha: 0.12 });
      const name = new BitmapText({
        text: `${z.name}  ·  ${z.count}`,
        style: { fontFamily: FONT, fontSize: zoneFont },
      });
      name.tint = PAL.zoneText;
      zonesLayer.addChild(g, name);
      zoneLabels.push({ text: name, zone: z });
    }
    placeZoneLabels();
  }

  /** Tamaño y posición de los rótulos de zona: centrados en su franja de 2 tiles. */
  function placeZoneLabels() {
    const t = layout.tile;
    for (const { text, zone } of zoneLabels) {
      text.style.fontSize = zoneFont;
      text.position.set(zone.x + t * 0.75, zone.y + Math.max(1, (2 * t - zoneFont * 1.15) / 2));
    }
  }

  function applyCamera(request = true) {
    cam = clampToBounds(cam, bounds);
    worldRoot.scale.set(cam.k);
    worldRoot.position.set(vp.w / 2 - cam.x * cam.k, vp.h / 2 - cam.y * cam.k);
    // Cada vista recalcula su texto solo si cambia el tamaño cuantizado (domain/text).
    for (const v of views.values()) v.setTextScale(cam.k);
    const nextZoneFont = fontFor(cam.k, 15, 6, 18);
    if (nextZoneFont !== zoneFont) {
      zoneFont = nextZoneFont;
      placeZoneLabels();
    }
    const nextLod = lodFor(cam.k, lod);
    if (nextLod !== lod) {
      lod = nextLod;
      for (const v of views.values()) v.setLod(lod);
      store.getState().setLod(lod);
    }
    if (request) scheduler?.request();
  }

  function setCamera(target: Camera, animate = false) {
    // La oficina bajo el ratón cambia al moverse la cámara: el tooltip ya no vale.
    store.getState().setHover(undefined);
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

  /** ¿La oficina (encendida o apagada) coincide con el filtro actual? */
  function roomMatches(id: string): boolean {
    const { filter, agents, dormant } = store.getState();
    const agent = agents.agents.get(id);
    if (agent) return matchesAgent(agent, filter);
    const conv = dormant.get(id);
    return conv ? matchesConversation(conv, filter) : false;
  }

  /** Atenúa las oficinas que no coinciden con el filtro (Hito 13). */
  function applyFilter() {
    const active = isActive(store.getState().filter);
    for (const [id, view] of views) view.setDimmed(active && !roomMatches(id));
  }

  /** Encuadre de las oficinas que coinciden con el filtro; todas si no hay filtro o ninguna. */
  function filterCamera(): Camera {
    if (!isActive(store.getState().filter)) return fitAllCamera();
    let box: Rect | undefined;
    for (const [id, r] of rooms) if (roomMatches(id)) box = box ? union(box, r) : r;
    return box ? fitInto(box, vp, insets(), FIT_PADDING, DEFAULT_LIMITS) : fitAllCamera();
  }

  /** Pantalla ocupada por la barra superior y, si está abierta, la terminal. */
  function insets() {
    const st = store.getState();
    const right = st.openTerminalId ? Math.round(vp.w * st.terminalFraction) + 12 : 0;
    return { top: TOP_INSET, right };
  }

  function fitAllCamera(): Camera {
    return fitInto(bounds, vp, insets(), FIT_PADDING, DEFAULT_LIMITS);
  }

  /**
   * Clic del usuario: selecciona y, si la oficina tiene terminal, la abre (ADR 0007). Una oficina
   * apagada solo se selecciona: su panel ofrece retomarla (ADR 0009).
   */
  function clickOffice(id: AgentId | undefined) {
    const st = store.getState();
    st.select(id);
    if (id !== undefined && isDormantId(id)) {
      st.closeTerminal();
      return;
    }
    const agent = id ? st.agents.agents.get(id) : undefined;
    const terminal = agent ? st.terminals.get(agent.pid) : undefined;
    if (terminal) st.openTerminal(terminal.id);
    else st.closeTerminal();
  }

  function officeAt(sx: number, sy: number): AgentId | undefined {
    const p = toWorld(cam, vp, { x: sx, y: sy });
    for (const [id, r] of rooms) {
      if (p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h) return id;
    }
    return undefined;
  }

  function resolveId(idOrIndex: string): AgentId | undefined {
    if (rooms.has(idOrIndex)) return idOrIndex;
    if (/^\d+$/.test(idOrIndex))
      return [...store.getState().agents.agents.keys()][Number(idOrIndex)];
    return undefined;
  }

  function focusId(id: AgentId) {
    const r = rooms.get(id);
    if (r === undefined) return;
    setCamera(fitInto(r, vp, insets(), FOCUS_PADDING, DEFAULT_LIMITS), true);
  }

  /** Oficina más cercana al centro del mundo, para los zooms mid y near. */
  function centralRect(): Rect {
    const cx = bounds.w / 2;
    const cy = bounds.h / 2;
    let best: Rect = { x: cx, y: cy, w: 0, h: 0 };
    let bestD = Number.POSITIVE_INFINITY;
    for (const r of rooms.values()) {
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
    const r = id === undefined ? undefined : rooms.get(id);
    cam = r === undefined ? zoomCamera(opts.zoom) : fitRect(r, vp, FOCUS_PADDING, DEFAULT_LIMITS);
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
    zoomStep: (factor) =>
      setCamera(zoomAt(cam, vp, { x: vp.w / 2, y: vp.h / 2 }, factor, DEFAULT_LIMITS), true),
    frames: () => scheduler?.frames ?? 0,
    screenCenter(idOrIndex) {
      const id = resolveId(idOrIndex);
      const r = id === undefined ? undefined : rooms.get(id);
      if (r === undefined) return undefined;
      return toScreen(cam, vp, { x: r.x + r.w / 2, y: r.y + r.h / 2 });
    },
    scale: () => cam.k,
    zones: () => zones.map((z) => ({ name: z.name, count: z.count })),
    dimmed: () => [...views].filter(([, v]) => v.root.alpha < 1).map(([id]) => id),
    board: (id) => views.get(id)?.boardShown(),
    mail: (id) => views.get(id)?.mailShown(),
    envelopes: () => envelopeCount,
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
