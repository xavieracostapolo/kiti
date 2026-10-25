import { describe, expect, it } from "vitest";
import { personFor } from "./people";

describe("personFor", () => {
  it("es estable: el mismo agente siempre es la misma persona", () => {
    const id = "claude:56950:1792886760000";
    expect(personFor(id, 6)).toBe(personFor(id, 6));
  });

  it("reparte entre todas las personas", () => {
    const seen = new Set<number>();
    for (let pid = 1000; pid < 1100; pid++) seen.add(personFor(`claude:${pid}:1`, 6));
    expect([...seen].sort()).toEqual([0, 1, 2, 3, 4, 5]);
  });

  it("siempre dentro del rango", () => {
    for (const id of ["", "x", "codex:1:2", "gemini:99999:1792886760000"]) {
      const n = personFor(id, 6);
      expect(n).toBeGreaterThanOrEqual(0);
      expect(n).toBeLessThan(6);
    }
  });
});
