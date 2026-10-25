// Telemetría de la UI sin dependencias (ADR 0004 §3, ficha de observabilidad del Hito 1).
// Se lee con window.__kiti.metrics() y en el panel de debug.

import type { Lod } from "../domain/lod";

/** Ventana deslizante de las últimas `size` muestras, con percentiles por rango más cercano. */
export class Histogram {
  private readonly samples: number[] = [];
  private next = 0;

  constructor(private readonly size: number) {}

  record(value: number): void {
    if (this.samples.length < this.size) this.samples.push(value);
    else this.samples[this.next] = value;
    this.next = (this.next + 1) % this.size;
  }

  get count(): number {
    return this.samples.length;
  }

  percentile(p: number): number {
    if (this.samples.length === 0) return 0;
    const sorted = [...this.samples].sort((a, b) => a - b);
    const rank = Math.max(1, Math.ceil((p / 100) * sorted.length));
    return sorted[rank - 1] as number;
  }

  summary(): { p50: number; p95: number } {
    return { p50: this.percentile(50), p95: this.percentile(95) };
  }
}

export type Gauges = { officesVisible: number; lod: Lod; tickerRunning: boolean };

export type MetricsSnapshot = {
  "ui.frames_total": number;
  "ui.frame_ms": { p50: number; p95: number };
  "ui.raf_delta_ms": { p50: number; p95: number };
  "ui.diff_to_paint_ms": { p50: number; p95: number };
  "ui.react_commits_total": number;
  "ui.offices_visible": number;
  "ui.lod": Lod;
  "ui.ticker_running": boolean;
};

export type Metrics = ReturnType<typeof createMetrics>;

export function createMetrics(window = 600) {
  const frameMs = new Histogram(window);
  const rafDelta = new Histogram(window);
  const diffToPaint = new Histogram(window);
  let frames = 0;
  let reactCommits = 0;
  let gauges: Gauges = { officesVisible: 0, lod: "far", tickerRunning: false };

  return {
    /** Un frame pintado: lo que tardó el render y el delta desde el frame anterior, si lo hubo. */
    frame(ms: number, delta?: number) {
      frames++;
      frameMs.record(ms);
      if (delta !== undefined) rafDelta.record(delta);
    },
    diffToPaint(ms: number) {
      diffToPaint.record(ms);
    },
    reactCommit() {
      reactCommits++;
    },
    setGauges(g: Partial<Gauges>) {
      gauges = { ...gauges, ...g };
    },
    snapshot(): MetricsSnapshot {
      return {
        "ui.frames_total": frames,
        "ui.frame_ms": frameMs.summary(),
        "ui.raf_delta_ms": rafDelta.summary(),
        "ui.diff_to_paint_ms": diffToPaint.summary(),
        "ui.react_commits_total": reactCommits,
        "ui.offices_visible": gauges.officesVisible,
        "ui.lod": gauges.lod,
        "ui.ticker_running": gauges.tickerRunning,
      };
    },
  };
}
