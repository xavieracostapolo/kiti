import { describe, expect, it } from "vitest";
import { charsIn, fitLines, fitWords, fontFor } from "./text";

describe("fontFor", () => {
  it("mantiene el tamaño en pantalla: más zoom, letra más pequeña en el mundo", () => {
    expect(fontFor(1, 12, 4, 9)).toBe(9); // lejos: el máximo
    expect(fontFor(3, 12, 4, 9)).toBe(4);
    expect(fontFor(6, 12, 4, 9)).toBe(4); // nunca menos del mínimo
  });

  it("se cuantiza a medios píxeles para no recalcular en cada frame del zoom", () => {
    expect(fontFor(2.1, 12, 4, 9)).toBe(5.5);
    expect(fontFor(2.12, 12, 4, 9)).toBe(5.5);
  });
});

describe("fitLines", () => {
  it("cabe en una línea tal cual", () => {
    expect(fitLines("Revisar vulnerabilidades", 30, 1)).toBe("Revisar vulnerabilidades");
  });

  it("parte por palabras", () => {
    expect(fitLines("revisa las dependencias con CVE", 14, 3)).toBe(
      "revisa las\ndependencias\ncon CVE",
    );
  });

  it("si no cabe, la última línea acaba en …", () => {
    expect(fitLines("revisa las dependencias con CVE abiertos", 14, 2)).toBe(
      "revisa las\ndependencias …",
    );
    expect(fitLines("Revisar vulnerabilidades", 10, 1)).toBe("Revisar v…");
  });

  it("corta las palabras más largas que una línea", () => {
    expect(fitLines("supercalifragilistico", 8, 3)).toBe("supercal\nifragili\nstico");
  });

  it("texto vacío", () => {
    expect(fitLines("", 10, 2)).toBe("");
  });
});

describe("fitWords", () => {
  it("baja la letra hasta que la palabra más larga cabe entera", () => {
    const size = fitWords("Revisar vulnerabilidades", 172, 20, 9);
    expect(size).toBeLessThan(20);
    expect(charsIn(172, size)).toBeGreaterThanOrEqual("vulnerabilidades".length);
    expect(fitWords("Hola", 172, 20, 9)).toBe(20);
    expect(fitWords("x".repeat(80), 172, 20, 9)).toBe(9);
  });
});
