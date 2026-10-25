// Resumen periódico de la UI hacia el core (ADR 0004 §3). Solo envía números: no pinta nada.
import type { UiMetrics } from "../protocol";
import { type Metrics, toUiMetrics } from "./metrics";

export function startUiReport(
  metrics: Metrics,
  report: (m: UiMetrics) => void,
  everyMs = 2000,
): () => void {
  let lastFrames = metrics.snapshot()["ui.frames_total"];
  const handle = setInterval(() => {
    const snap = metrics.snapshot();
    const frames = snap["ui.frames_total"];
    report(toUiMetrics(snap, frames - lastFrames, everyMs));
    lastFrames = frames;
  }, everyMs);
  return () => clearInterval(handle);
}
