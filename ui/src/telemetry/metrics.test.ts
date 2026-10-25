import { describe, expect, it } from "vitest";
import { createMetrics, Histogram } from "./metrics";

describe("Histogram", () => {
  it("percentiles por rango más cercano", () => {
    const h = new Histogram(100);
    for (let i = 1; i <= 100; i++) h.record(i);
    expect(h.percentile(50)).toBe(50);
    expect(h.percentile(95)).toBe(95);
    expect(h.percentile(100)).toBe(100);
  });

  it("vacío devuelve 0", () => {
    expect(new Histogram(10).percentile(95)).toBe(0);
  });

  it("ventana deslizante: solo cuenta las últimas N muestras", () => {
    const h = new Histogram(3);
    for (const v of [100, 100, 100, 1, 2, 3]) h.record(v);
    expect(h.percentile(100)).toBe(3);
    expect(h.count).toBe(3);
  });
});

describe("createMetrics", () => {
  it("expone las señales de la ficha de observabilidad del Hito 1", () => {
    const m = createMetrics();
    m.frame(4, 16);
    m.frame(6, 17);
    m.diffToPaint(12);
    m.reactCommit();
    m.setGauges({ officesVisible: 7, lod: "mid", tickerRunning: false });
    expect(m.snapshot()).toEqual({
      "ui.frames_total": 2,
      "ui.frame_ms": { p50: 4, p95: 6 },
      "ui.raf_delta_ms": { p50: 16, p95: 17 },
      "ui.diff_to_paint_ms": { p50: 12, p95: 12 },
      "ui.react_commits_total": 1,
      "ui.offices_visible": 7,
      "ui.lod": "mid",
      "ui.ticker_running": false,
    });
  });
});
