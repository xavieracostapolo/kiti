// Nivel de detalle con histéresis (ADR 0003 §5). far: sala y semáforo; mid: + nombre;
// near: + interior, avatar y última herramienta.

export type Lod = "far" | "mid" | "near";

// [subir a este nivel desde abajo, bajar desde este nivel]
const MID = { up: 1.7, down: 1.5 };
const NEAR = { up: 3.2, down: 2.8 };

export function lodFor(scale: number, prev: Lod): Lod {
  switch (prev) {
    case "far":
      if (scale >= NEAR.up) return "near";
      return scale >= MID.up ? "mid" : "far";
    case "mid":
      if (scale >= NEAR.up) return "near";
      return scale < MID.down ? "far" : "mid";
    case "near":
      if (scale >= NEAR.down) return "near";
      return scale < MID.down ? "far" : "mid";
  }
}

/** Escala representativa de cada nivel, para `?zoom=mid|near`. */
export const LOD_SCALE: Record<Exclude<Lod, "far">, number> = { mid: 2.2, near: 4 };
