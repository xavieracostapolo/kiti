// Plantilla estática de una oficina, dibujada una vez y rasterizada a una textura que comparten
// todas las oficinas (ADR 0003 §5). Procedural mientras no haya tilesheets de Kenney.

import { Container, Graphics } from "pixi.js";
import { PAL } from "./palette";

export type RoomSize = { w: number; h: number; tile: number };

/** Anclas en coordenadas locales de la sala, para lo dinámico. */
export function anchors({ w, tile }: RoomSize) {
  return {
    light: { x: w - tile * 1.5, y: 3 },
    label: { x: tile * 0.75, y: 4 },
    badge: { x: tile * 0.5, y: tile * 2.5 },
    desk: { x: tile * 4, y: tile * 4 },
    avatar: { x: tile * 5.5, y: tile * 5.6 },
    bubble: { x: tile * 6.5, y: tile * 3.1 },
  };
}

export function drawOfficeTemplate({ w, h, tile }: RoomSize): Container {
  const root = new Container();
  const g = new Graphics();
  root.addChild(g);

  // Suelo de madera en damero.
  for (let ty = 2; ty < h / tile; ty++) {
    for (let tx = 0; tx < w / tile; tx++) {
      g.rect(tx * tile, ty * tile, tile, tile).fill((tx + ty) % 2 ? PAL.floorA : PAL.floorB);
    }
  }
  // Vetas.
  for (let ty = 2; ty < h / tile; ty++) {
    g.rect(0, ty * tile + tile - 1, w, 1).fill({ color: 0x000000, alpha: 0.08 });
  }

  // Pared superior con zócalo.
  g.rect(0, 0, w, tile * 2).fill(PAL.wall);
  g.rect(0, tile * 2 - 3, w, 3).fill(PAL.wallTrim);

  // Ventana: estrecha y a la derecha, para dejar sitio al título de la conversación (ADR 0009).
  const wx = tile * 7.75;
  g.rect(wx - 2, 6, tile * 2 + 4, tile + 8).fill(PAL.windowFrame);
  g.rect(wx, 8, tile * 2, tile + 4).fill(PAL.window);
  g.rect(wx + tile - 1, 8, 2, tile + 4).fill(PAL.windowFrame);
  g.rect(wx + 3, 11, 5, 2).fill(PAL.windowShine);
  g.rect(wx + 3, 14, 3, 2).fill(PAL.windowShine);

  // Alfombra.
  g.rect(tile * 2.5, tile * 6.5, tile * 6, tile * 2).fill(PAL.rugEdge);
  g.rect(tile * 2.5 + 2, tile * 6.5 + 2, tile * 6 - 4, tile * 2 - 4).fill(PAL.rug);

  // Escritorio con monitor (la pantalla es dinámica) y silla.
  const dx = tile * 4;
  const dy = tile * 4;
  g.rect(dx, dy, tile * 3, tile).fill(PAL.deskTop);
  g.rect(dx, dy + tile, tile * 3, 5).fill(PAL.deskSide);
  g.rect(dx + 2, dy + tile + 5, 3, 6).fill(PAL.deskSide);
  g.rect(dx + tile * 3 - 5, dy + tile + 5, 3, 6).fill(PAL.deskSide);
  g.rect(dx + tile - 2, dy - 9, tile + 4, 11).fill(PAL.monitor);
  g.rect(dx + tile * 1.5 - 1, dy + 1, 2, 3).fill(PAL.monitor);
  g.rect(dx + tile * 1.5 - 4, dy + 3, 8, 2).fill(PAL.monitor);
  g.rect(dx + tile * 2.4, dy + 4, 6, 4).fill(0xf5f5f5); // papeles
  g.rect(dx + tile * 1.1, dy + tile * 2.1, tile * 0.8, tile * 0.7).fill(PAL.chair);

  // Estantería con libros.
  const sx = tile * 9;
  g.rect(sx, tile * 2 + 2, tile * 2, tile * 2.5).fill(PAL.shelf);
  for (let row = 0; row < 3; row++) {
    for (let b = 0; b < 5; b++) {
      g.rect(sx + 3 + b * 5, tile * 2 + 5 + row * 12, 4, 9).fill(
        PAL.books[(row + b) % PAL.books.length] as number,
      );
    }
  }

  // Planta.
  const px = tile * 0.6;
  const py = tile * 7.2;
  g.rect(px + 3, py + 8, 10, 8).fill(PAL.potA);
  g.circle(px + 8, py + 4, 7).fill(PAL.leafB);
  g.circle(px + 5, py + 2, 4).fill(PAL.leafA);
  g.circle(px + 11, py + 3, 4).fill(PAL.leafA);

  // Puerta (hueco en la pared inferior) y contorno.
  g.rect(0, 0, w, 2).fill(PAL.outline);
  g.rect(0, h - 2, tile * 5, 2).fill(PAL.outline);
  g.rect(tile * 7, h - 2, w - tile * 7, 2).fill(PAL.outline);
  g.rect(0, 0, 2, h).fill(PAL.outline);
  g.rect(w - 2, 0, 2, h).fill(PAL.outline);

  return root;
}
