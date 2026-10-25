// Conecta los eventos del DOM con la cámara (ADR 0003 §5). La traducción está en inputMapping.ts.

import { gestureFactor, keyAction, wheelAction } from "./inputMapping";

export type CameraController = {
  pan(dx: number, dy: number): void;
  zoomAt(sx: number, sy: number, factor: number): void;
  zoomCenter(factor: number): void;
  fitAll(): void;
  click(sx: number, sy: number): void;
  focusAt(sx: number, sy: number): void;
  dragging(active: boolean): void;
};

const DRAG_THRESHOLD_PX = 3;

type GestureLike = Event & { scale: number; clientX?: number; clientY?: number };

export function attachCameraInput(el: HTMLElement, ctl: CameraController): () => void {
  let down: { id: number; x: number; y: number; lastX: number; lastY: number } | undefined;
  let dragging = false;
  let gestureScale = 1;

  const local = (clientX: number, clientY: number) => {
    const r = el.getBoundingClientRect();
    return { x: clientX - r.left, y: clientY - r.top };
  };

  const onPointerDown = (e: PointerEvent) => {
    if (e.button !== 0) return;
    el.setPointerCapture(e.pointerId);
    down = { id: e.pointerId, x: e.clientX, y: e.clientY, lastX: e.clientX, lastY: e.clientY };
    dragging = false;
  };

  const onPointerMove = (e: PointerEvent) => {
    if (!down || e.pointerId !== down.id) return;
    if (!dragging && Math.hypot(e.clientX - down.x, e.clientY - down.y) > DRAG_THRESHOLD_PX) {
      dragging = true;
      ctl.dragging(true);
    }
    if (dragging) ctl.pan(e.clientX - down.lastX, e.clientY - down.lastY);
    down.lastX = e.clientX;
    down.lastY = e.clientY;
  };

  const onPointerUp = (e: PointerEvent) => {
    if (!down || e.pointerId !== down.id) return;
    if (dragging) ctl.dragging(false);
    else {
      const p = local(e.clientX, e.clientY);
      ctl.click(p.x, p.y);
    }
    down = undefined;
    dragging = false;
  };

  const onWheel = (e: WheelEvent) => {
    e.preventDefault();
    const action = wheelAction(e);
    if (action.kind === "pan") ctl.pan(action.dx, action.dy);
    else if (action.kind === "zoom") {
      const p = local(e.clientX, e.clientY);
      ctl.zoomAt(p.x, p.y, action.factor);
    }
  };

  const onDblClick = (e: MouseEvent) => {
    const p = local(e.clientX, e.clientY);
    ctl.focusAt(p.x, p.y);
  };

  // Pinch del trackpad en WebKit / WKWebView.
  const onGestureStart = (e: Event) => {
    e.preventDefault();
    gestureScale = (e as GestureLike).scale || 1;
  };
  const onGestureChange = (e: Event) => {
    e.preventDefault();
    const g = e as GestureLike;
    const factor = gestureFactor(gestureScale, g.scale);
    gestureScale = g.scale;
    if (g.clientX !== undefined && g.clientY !== undefined) {
      const p = local(g.clientX, g.clientY);
      ctl.zoomAt(p.x, p.y, factor);
    } else ctl.zoomCenter(factor);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const target = e.target as HTMLElement | null;
    if (target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName)))
      return;
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const action = keyAction(e.key);
    if (!action) return;
    if (action.kind === "fitAll") ctl.fitAll();
    else if (action.kind === "zoomStep") ctl.zoomCenter(action.factor);
  };

  el.addEventListener("pointerdown", onPointerDown);
  el.addEventListener("pointermove", onPointerMove);
  el.addEventListener("pointerup", onPointerUp);
  el.addEventListener("pointercancel", onPointerUp);
  el.addEventListener("wheel", onWheel, { passive: false });
  el.addEventListener("dblclick", onDblClick);
  el.addEventListener("gesturestart", onGestureStart);
  el.addEventListener("gesturechange", onGestureChange);
  window.addEventListener("keydown", onKeyDown);

  return () => {
    el.removeEventListener("pointerdown", onPointerDown);
    el.removeEventListener("pointermove", onPointerMove);
    el.removeEventListener("pointerup", onPointerUp);
    el.removeEventListener("pointercancel", onPointerUp);
    el.removeEventListener("wheel", onWheel);
    el.removeEventListener("dblclick", onDblClick);
    el.removeEventListener("gesturestart", onGestureStart);
    el.removeEventListener("gesturechange", onGestureChange);
    window.removeEventListener("keydown", onKeyDown);
  };
}
