// Traducción pura de eventos de entrada a acciones de cámara (ADR 0003 §5). Sin DOM ni Pixi.

export type WheelLike = {
  deltaX: number;
  deltaY: number;
  deltaMode: number;
  ctrlKey: boolean;
  metaKey: boolean;
};

export type CameraAction =
  | { kind: "pan"; dx: number; dy: number }
  | { kind: "zoom"; factor: number }
  | { kind: "zoomStep"; factor: number }
  | { kind: "fitAll" };

const LINE_PX = 16;
const ZOOM_SENSITIVITY = 0.0025;

export function wheelAction(e: WheelLike): CameraAction {
  const unit = e.deltaMode === 1 ? LINE_PX : 1;
  const dx = e.deltaX * unit;
  const dy = e.deltaY * unit;
  if (e.ctrlKey || e.metaKey) return { kind: "zoom", factor: Math.exp(-dy * ZOOM_SENSITIVITY) };
  return { kind: "pan", dx: -dx, dy: -dy };
}

export function gestureFactor(prevScale: number, scale: number): number {
  return prevScale > 0 && scale > 0 ? scale / prevScale : 1;
}

export function keyAction(key: string): CameraAction | undefined {
  switch (key) {
    case "0":
    case "Escape":
      return { kind: "fitAll" };
    case "+":
    case "=":
      return { kind: "zoomStep", factor: 1.25 };
    case "-":
      return { kind: "zoomStep", factor: 0.8 };
    default:
      return undefined;
  }
}
