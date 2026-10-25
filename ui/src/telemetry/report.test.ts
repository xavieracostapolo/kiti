import { afterEach, describe, expect, it, vi } from "vitest";
import { createMetrics } from "./metrics";
import { startUiReport } from "./report";

describe("startUiReport", () => {
  afterEach(() => vi.useRealTimers());

  it("envía un UiMetrics cada 2 s con los frames de esa ventana", () => {
    vi.useFakeTimers();
    const metrics = createMetrics();
    const report = vi.fn();
    const stop = startUiReport(metrics, report, 2000);

    metrics.frame(3, undefined);
    metrics.frame(3, 16);
    vi.advanceTimersByTime(2000);
    expect(report).toHaveBeenCalledTimes(1);
    expect(report.mock.calls[0]?.[0]).toMatchObject({ idleFrames: 2, fps: 1 });

    vi.advanceTimersByTime(2000); // reposo: 0 frames
    expect(report.mock.calls[1]?.[0]).toMatchObject({ idleFrames: 0, fps: 0 });

    stop();
    vi.advanceTimersByTime(10_000);
    expect(report).toHaveBeenCalledTimes(2);
  });
});
