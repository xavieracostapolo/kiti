import { describe, expect, it } from "vitest";
import { DEFAULT_LAYOUT } from "./layout";
import { layoutZones, projectName, type RoomEntry } from "./zones";

const T = DEFAULT_LAYOUT.tile;
const entries = (...pairs: [string, string][]): RoomEntry[] =>
  pairs.map(([id, project]) => ({ id, project }));

describe("layoutZones", () => {
  it("agrupa por proyecto y ordena las zonas por nombre", () => {
    const l = layoutZones(
      new Map(),
      entries(["a", "/w/tienda-web"], ["b", "/w/kiti"], ["c", "/w/tienda-web"]),
      DEFAULT_LAYOUT,
    );
    expect(l.zones.map((z) => [z.name, z.count])).toEqual([
      ["kiti", 1],
      ["tienda-web", 2],
    ]);
    // Cada oficina queda dentro de su zona, bajo el rótulo.
    const tienda = l.zones[1];
    for (const id of ["a", "c"]) {
      const r = l.rooms.get(id);
      expect(r).toBeDefined();
      if (!r || !tienda) continue;
      expect(r.x).toBeGreaterThanOrEqual(tienda.x);
      expect(r.x + r.w).toBeLessThanOrEqual(tienda.x + tienda.w);
      expect(r.y).toBeGreaterThanOrEqual(tienda.y + 2 * T);
      expect(r.y + r.h).toBeLessThanOrEqual(tienda.y + tienda.h);
    }
  });

  it("ninguna oficina se solapa con otra", () => {
    const many = Array.from({ length: 23 }, (_, i): [string, string] => [`id${i}`, `/w/p${i % 6}`]);
    const rects = [...layoutZones(new Map(), entries(...many), DEFAULT_LAYOUT).rooms.values()];
    for (const [i, a] of rects.entries()) {
      for (const b of rects.slice(i + 1)) {
        const apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
        expect(apart).toBe(true);
      }
    }
  });

  it("las zonas que no caben en la fila pasan a la siguiente", () => {
    const l = layoutZones(
      new Map(),
      entries(
        ...Array.from({ length: 9 }, (_, i): [string, string] => [`a${i}`, "/w/a"]),
        ["b", "/w/b"],
        ["c", "/w/c"],
        ["d", "/w/d"],
      ),
      DEFAULT_LAYOUT,
      { cols: 3, header: 2, pad: 1, gap: 3, maxRowRooms: 6 },
    );
    const [a, b, c, d] = l.zones;
    expect(b?.y).toBe(a?.y);
    expect(c?.y).toBe(a?.y);
    expect(d?.x).toBe(0);
    expect(d?.y).toBeGreaterThan((a?.y ?? 0) + (a?.h ?? 0));
    expect(l.bounds.h).toBe((d?.y ?? 0) + (d?.h ?? 0));
  });

  it("si una oficina se va, las demás de su zona no se mueven", () => {
    const first = layoutZones(
      new Map(),
      entries(["a", "/w/x"], ["b", "/w/x"], ["c", "/w/x"]),
      DEFAULT_LAYOUT,
    );
    const next = layoutZones(first.slots, entries(["a", "/w/x"], ["c", "/w/x"]), DEFAULT_LAYOUT);
    expect(next.rooms.get("c")).toEqual(first.rooms.get("c"));
    expect(next.rooms.get("a")).toEqual(first.rooms.get("a"));
    expect(next.rooms.has("b")).toBe(false);
  });

  it("sin oficinas el mundo mide una sala", () => {
    const l = layoutZones(new Map(), [], DEFAULT_LAYOUT);
    expect(l.zones).toEqual([]);
    expect(l.bounds).toEqual({
      x: 0,
      y: 0,
      w: DEFAULT_LAYOUT.roomW * T,
      h: DEFAULT_LAYOUT.roomH * T,
    });
  });
});

describe("projectName", () => {
  it("es la última carpeta", () => {
    expect(projectName("/Users/dev/tienda-web")).toBe("tienda-web");
    expect(projectName("/Users/dev/tienda-web/")).toBe("tienda-web");
    expect(projectName("/")).toBe("/");
  });
});
