import { describe, expect, it } from "vitest";
import type { QuotaWindow } from "../protocol";
import { current, levelOf, nextReset, summarize } from "./quota";

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

  it("una ventana ya reiniciada vuelve al 100 % aunque no haya llegado otra lectura", () => {
    const at = (resetsAt: number): QuotaWindow => ({ ...w("5 h", false, 30), resetsAt });
    expect(current([at(1000)], 999)[0]?.remainingPct).toBe(30);
    expect(current([at(1000)], 1000)[0]?.remainingPct).toBe(100);
    // Sin hora de reinicio conocida (0) se queda como llegó.
    expect(current([at(0)], 5000)[0]?.remainingPct).toBe(30);
  });

  it("el próximo reinicio es el más cercano que aún no ha pasado", () => {
    const at = (resetsAt: number): QuotaWindow => ({ ...w("x", false, 30), resetsAt });
    expect(nextReset([at(0), at(500), at(3000), at(2000)], 1000)).toBe(2000);
    expect(nextReset([at(0), at(500)], 1000)).toBeUndefined();
  });
});
