import fc from "fast-check";
import { describe, expect, it } from "vitest";
import { assignSlots, type LayoutOpts, slotRect, worldBounds } from "./layout";

const opts: LayoutOpts = { cols: 6, roomW: 12, roomH: 9, gap: 2, tile: 16 };

describe("assignSlots", () => {
  it("un agente nuevo ocupa el hueco libre más bajo y nadie se mueve", () => {
    const prev = new Map([
      ["a", 0],
      ["b", 1],
      ["c", 2],
    ]);
    const next = assignSlots(prev, ["a", "c", "d"]); // b murió
    expect(next).toEqual(
      new Map([
        ["a", 0],
        ["c", 2],
        ["d", 1],
      ]),
    );
  });

  it("los ids nuevos se asignan en el orden en que llegan", () => {
    expect(assignSlots(new Map(), ["x", "y", "z"])).toEqual(
      new Map([
        ["x", 0],
        ["y", 1],
        ["z", 2],
      ]),
    );
  });

  it("propiedad: los supervivientes conservan su slot y no hay slots repetidos", () => {
    const ids = fc.uniqueArray(fc.string({ minLength: 1, maxLength: 4 }), { maxLength: 40 });
    fc.assert(
      fc.property(ids, ids, (first, second) => {
        const prev = assignSlots(new Map(), first);
        const next = assignSlots(prev, second);
        for (const id of second) {
          if (prev.has(id)) expect(next.get(id)).toBe(prev.get(id));
        }
        const slots = [...next.values()];
        expect(new Set(slots).size).toBe(slots.length);
        expect(next.size).toBe(second.length);
      }),
    );
  });
});

describe("slotRect", () => {
  it("convierte un slot en un rectángulo del mundo", () => {
    // slot 7 → columna 1, fila 1; paso = (12+2)·16 = 224 en x y (9+2)·16 = 176 en y
    expect(slotRect(7, opts)).toEqual({ x: 224, y: 176, w: 192, h: 144 });
  });

  it("propiedad: dos slots distintos nunca se solapan", () => {
    const slot = fc.nat({ max: 200 });
    fc.assert(
      fc.property(slot, slot, (s1, s2) => {
        fc.pre(s1 !== s2);
        const a = slotRect(s1, opts);
        const b = slotRect(s2, opts);
        const overlap = a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
        expect(overlap).toBe(false);
      }),
    );
  });
});

describe("worldBounds", () => {
  it("cubre todas las oficinas ocupadas", () => {
    const slots = new Map([
      ["a", 0],
      ["b", 7],
    ]);
    expect(worldBounds(slots, opts)).toEqual({ x: 0, y: 0, w: 224 + 192, h: 176 + 144 });
  });

  it("sin oficinas devuelve un rectángulo de una sala", () => {
    expect(worldBounds(new Map(), opts)).toEqual({ x: 0, y: 0, w: 192, h: 144 });
  });
});
