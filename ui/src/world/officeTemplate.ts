// Plantilla estática de una oficina con tiles de Kenney (ADR 0013): se dibuja una vez por proveedor
// (cambia la alfombra) y se rasteriza a una textura que comparten todas sus oficinas (ADR 0003 §5).
// Lo funcional (semáforo, pantalla del monitor, títulos, insignias) lo pinta OfficeView encima.

import { Container, Graphics, Sprite } from "pixi.js";
import type { Provider } from "../protocol";
import { RUG, type Sheets, TILES, type TileRef, tileTexture } from "./kenney";
import { PAL } from "./palette";

export type RoomSize = { w: number; h: number; tile: number };

export const DECOR_VARIANTS = 3;

/** Anclas en coordenadas locales de la sala, para lo dinámico. */
export function anchors({ w, tile }: RoomSize) {
  return {
    light: { x: w - tile * 1.5, y: 3 },
    label: { x: tile * 1.65, y: 5 },
    badge: { x: 6, y: 6 },
    desk: { x: tile * 4, y: tile * 4 },
    /** Esquina superior izquierda de la persona (un tile), sentada de espaldas al monitor. */
    person: { x: tile * 5, y: tile * 5 },
    bubble: { x: tile * 6.5, y: tile * 3.1 },
  };
}

export function drawOfficeTemplate(
  { w, h, tile }: RoomSize,
  sheets: Sheets,
  provider: Provider,
  variant = 0,
): Container {
  const root = new Container();
  const put = ([sheet, col, row]: TileRef, x: number, y: number, tint?: number) => {
    const s = new Sprite(tileTexture(sheets, sheet, col, row));
    s.position.set(x, y);
    if (tint !== undefined) s.tint = tint;
    root.addChild(s);
  };
  const cols = w / tile;
  const rows = h / tile;

  // Pared de dos filas y suelo de madera.
  for (let x = 0; x < cols; x++) {
    put(TILES.wallUpper, x * tile, 0);
    put(TILES.wallLower, x * tile, tile);
    for (let y = 2; y < rows; y++) put(TILES.floor, x * tile, y * tile);
  }

  // Alfombra del color del proveedor bajo la silla (Antigravity se tinta de violeta suave).
  const [rc, rr] = RUG[provider];
  const rugTint = provider === "antigravity" ? 0xd4b5ff : undefined;
  for (let dx = 0; dx < 3; dx++) {
    for (let dy = 0; dy < 3; dy++) {
      put(["rpg", rc + dx, rr + dy], (4 + dx) * tile, (5 + dy) * tile, rugTint);
    }
  }

  // Cuadro en la pared y estantería según la variante de decoración.
  const v = variant % DECOR_VARIANTS;
  if (v === 1) {
    put(TILES.paintingAltA, tile * 8.2, 8);
    put(TILES.paintingAltB, tile * 9.2, 8);
    put(TILES.shelfA, tile * 1, tile * 2);
    put(TILES.shelfB, tile * 2, tile * 2);
    put(TILES.shelfC, tile * 3, tile * 2);
    put(TILES.roundTable, tile * 9.5, tile * 4.5);
    put(TILES.plantB, tile * 1.2, tile * 6.8);
    put(TILES.plantA, tile * 9.8, tile * 6.8);
  } else if (v === 2) {
    put(TILES.paintingA, tile * 8.2, 8);
    put(TILES.paintingB, tile * 9.2, 8);
    put(TILES.shelfA, tile * 8, tile * 2);
    put(TILES.shelfB, tile * 9, tile * 2);
    put(TILES.shelfC, tile * 10, tile * 2);
    put(TILES.roundTable, tile * 1.5, tile * 4.5);
    put(TILES.plantA, tile * 1.5, tile * 2.3);
    put(TILES.plantB, tile * 10, tile * 7);
  } else {
    put(TILES.paintingA, tile * 8.2, 8);
    put(TILES.paintingB, tile * 9.2, 8);
    put(TILES.shelfA, tile * 8, tile * 2);
    put(TILES.shelfB, tile * 9, tile * 2);
    put(TILES.shelfC, tile * 10, tile * 2);
    put(TILES.plantA, tile, tile * 7);
    put(TILES.plantB, tile * 10, tile * 7);
  }

  // Escritorio, silla y monitor (la pantalla, con el color del estado, la pinta OfficeView).
  put(TILES.deskLeft, tile * 4, tile * 4);
  put(TILES.deskMid, tile * 5, tile * 4);
  put(TILES.deskRight, tile * 6, tile * 4);
  put(TILES.chair, tile * 5, tile * 5);
  const monitor = new Graphics()
    .rect(tile * 5 - 2, tile * 4 - 9, tile + 4, 11)
    .fill(PAL.monitor)
    .rect(tile * 5.5 - 1, tile * 4 + 1, 2, 3)
    .fill(PAL.monitor);
  root.addChild(monitor);
  return root;
}
