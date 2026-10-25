// Tiles de Kenney (CC0) que usa la oficina (ADR 0013). Las hojas tienen tiles de 16 px con 1 px de
// separación; aquí solo hay coordenadas (columna, fila) y la carga de texturas.

import { Rectangle, Texture } from "pixi.js";
import type { Provider } from "../protocol";

const BASE = `${import.meta.env.BASE_URL}assets/kenney`;

export const SHEETS = {
  /** Roguelike RPG Pack: pared, suelo y alfombras. */
  rpg: `${BASE}/roguelike-rpg-pack/sheet.png`,
  /** Roguelike Indoors: muebles. */
  indoors: `${BASE}/roguelike-indoors/sheet.png`,
  /** RPG Urban Pack: personas. */
  urban: `${BASE}/rpg-urban-pack/sheet.png`,
} as const;

export type Sheet = keyof typeof SHEETS;
export type TileRef = readonly [Sheet, number, number];

export const TILES = {
  wallUpper: ["rpg", 18, 15],
  wallLower: ["rpg", 13, 15],
  floor: ["rpg", 5, 2],
  deskLeft: ["indoors", 0, 0],
  deskMid: ["indoors", 1, 0],
  deskRight: ["indoors", 2, 0],
  chair: ["indoors", 1, 2],
  shelfA: ["indoors", 19, 17],
  shelfB: ["indoors", 20, 17],
  shelfC: ["indoors", 21, 17],
  paintingA: ["indoors", 20, 12],
  paintingB: ["indoors", 21, 12],
  plantA: ["indoors", 16, 0],
  plantB: ["indoors", 17, 0],
} as const satisfies Record<string, TileRef>;

/** Alfombra 3×3 del color de cada proveedor: esquina superior izquierda en la hoja `rpg`. */
export const RUG: Record<Provider, readonly [number, number]> = {
  claude: [10, 19],
  codex: [10, 16],
  gemini: [10, 22],
};

/** Personas del RPG Urban Pack, vistas de espaldas (columna 25): primera fila de cada una. */
export const PEOPLE_BACK_COLUMN = 25;
export const PEOPLE_ROWS = [0, 3, 6, 9, 12, 15] as const;

export type Sheets = Record<Sheet, Texture>;

/**
 * Como `<img>`, no con `fetch` (lo que hace `Assets.load`): la CSP de la app solo deja `connect-src`
 * para el IPC, y las imágenes del propio origen ya están permitidas por `img-src 'self'`.
 */
async function loadImage(url: string): Promise<Texture> {
  const img = new Image();
  img.src = url;
  await img.decode();
  return Texture.from(img);
}

/** Carga las tres hojas una vez; los tiles son vistas (sin copiar píxeles). */
export async function loadSheets(): Promise<Sheets> {
  const [rpg, indoors, urban] = await Promise.all([
    loadImage(SHEETS.rpg),
    loadImage(SHEETS.indoors),
    loadImage(SHEETS.urban),
  ]);
  return { rpg, indoors, urban };
}

export function tileTexture(sheets: Sheets, sheet: Sheet, col: number, row: number): Texture {
  return new Texture({
    source: sheets[sheet].source,
    frame: new Rectangle(col * 17, row * 17, 16, 16),
  });
}
