// Paleta pixel-art de la oficina y colores del semáforo. Un solo lugar para iterar el estilo.
import type { Provider, StatusKind } from "../protocol";

export const STATUS_COLOR: Record<StatusKind, number> = {
  working: 0x3ddc84,
  waiting: 0xffc93c,
  idle: 0x8a94a6,
  error: 0xff5c5c,
  dead: 0x7a1f2b,
};

export const PROVIDER_COLOR: Record<Provider, number> = {
  claude: 0xd97757,
  codex: 0x10a37f,
  gemini: 0x4285f4,
};

export const PROVIDER_LETTER: Record<Provider, string> = { claude: "C", codex: "X", gemini: "G" };

export const PAL = {
  background: 0x1a1d29,
  corridor: 0x232838,
  corridorDot: 0x2b3145,
  outline: 0x0e1018,
  floorA: 0xc9a47c,
  floorB: 0xbd976f,
  wall: 0x4a5a78,
  wallTrim: 0x34405a,
  window: 0x9fd3ff,
  windowShine: 0xd9f0ff,
  windowFrame: 0x2a3348,
  deskTop: 0x8d5d38,
  deskSide: 0x6b4426,
  monitor: 0x1c1f27,
  chair: 0x3b4252,
  rug: 0x6d8bb5,
  rugEdge: 0x56719a,
  potA: 0xa0522d,
  leafA: 0x4caf50,
  leafB: 0x2e7d32,
  shelf: 0x5c3b22,
  books: [0xe57373, 0x64b5f6, 0xffd54f, 0x81c784] as const,
  skin: 0xf1c27d,
  hair: 0x3e2723,
  lampOff: 0x2a2f3d,
  housing: 0x14171f,
  text: 0xf5f7fa,
  bubble: 0xffffff,
  bubbleText: 0x1a1d29,
  selection: 0xffffff,
  /** Borde de una oficina apagada (conversación cerrada, ADR 0009). */
  dormant: 0x4a5266,
  /** Zona de un proyecto: suelo, borde y rótulo. */
  zoneFloor: 0x2a3042,
  zoneBorder: 0x3a4258,
  zoneHeader: 0x323a50,
  zoneText: 0xc9d1e3,
} as const;
