import { describe, expect, it } from "vitest";
import type { QuotaWindow } from "../protocol";
import { levelOf, summarize } from "./quota";

const w = (label: string, weekly: boolean, remainingPct: number): QuotaWindow => ({
  label,
  weekly,
  remainingPct,
  resetsAt: 0,
});

describe("cuota", () => {
  it("niveles: verde, amarillo y rojo desde el 20 % (como el aviso)", () => {
    expect(levelOf(80)).toBe("ok");
    expect(levelOf(50)).toBe("warn");
    expect(levelOf(21)).toBe("warn");
    expect(levelOf(20)).toBe("low");
  });

  it("resume con la peor ventana corta y la peor semanal", () => {
    const s = summarize([
      w("Gemini · semana", true, 100),
      w("Claude and GPT models · semana", true, 48),
      w("Gemini · 5 h", false, 100),
      w("Claude and GPT models · 5 h", false, 90),
    ]);
    expect(s).toEqual({ short: 90, weekly: 48, level: "warn" });
    expect(summarize([w("semana", true, 12)])).toEqual({ weekly: 12, level: "low" });
    expect(summarize([])).toEqual({ level: "ok" });
  });
});
