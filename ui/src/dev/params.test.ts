import { describe, expect, it } from "vitest";
import { parseParams } from "./params";

describe("parseParams", () => {
  it("sin parámetros usa sim=20&seed=1 y animaciones activas", () => {
    expect(parseParams("")).toEqual({
      sim: 20,
      seed: 1,
      steps: 0,
      freeze: false,
      anim: true,
      zoom: "far",
      terminals: false,
    });
  });

  it("lee todos los parámetros del ciclo visual", () => {
    expect(
      parseParams("?sim=50&seed=42&steps=120&freeze=1&anim=0&zoom=near&focus=3&bench=pan"),
    ).toEqual({
      sim: 50,
      seed: 42,
      steps: 120,
      freeze: true,
      anim: false,
      zoom: "near",
      focus: "3",
      bench: "pan",
      terminals: false,
    });
  });

  it("ignora valores inválidos y acota sim", () => {
    expect(parseParams("?sim=abc&seed=-&zoom=lejos&steps=-5")).toMatchObject({
      sim: 20,
      seed: 1,
      zoom: "far",
      steps: 0,
    });
    expect(parseParams("?sim=5000").sim).toBe(200);
  });
});
