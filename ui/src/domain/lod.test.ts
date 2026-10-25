import { describe, expect, it } from "vitest";
import { lodFor } from "./lod";

describe("lodFor con histéresis", () => {
  it("elige el nivel por escala", () => {
    expect(lodFor(1, "far")).toBe("far");
    expect(lodFor(2.2, "far")).toBe("mid");
    expect(lodFor(4, "far")).toBe("near");
  });

  it("no oscila cerca del umbral far/mid", () => {
    expect(lodFor(1.6, "far")).toBe("far"); // aún no supera 1.7 para subir
    expect(lodFor(1.6, "mid")).toBe("mid"); // aún no baja de 1.5
    expect(lodFor(1.4, "mid")).toBe("far");
  });

  it("no oscila cerca del umbral mid/near", () => {
    expect(lodFor(3, "mid")).toBe("mid");
    expect(lodFor(3, "near")).toBe("near");
    expect(lodFor(2.7, "near")).toBe("mid");
  });
});
