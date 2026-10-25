import fc from "fast-check";
import { describe, expect, it } from "vitest";
import { assignSlots } from "./layout";

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
