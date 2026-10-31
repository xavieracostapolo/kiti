// Colores de lo que se dibuja por código (semáforo, monitor, insignias, zonas). La sala usa tiles
// de Kenney (ADR 0013). Un solo lugar para iterar el estilo.
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
  antigravity: 0x8b5cf6,
};

export const PROVIDER_LETTER: Record<Provider, string> = {
  claude: "C",
  codex: "X",
  gemini: "G",
  antigravity: "A",
};

export const PAL = {
  background: 0x1a1d29,
  corridor: 0x232838,
  corridorDot: 0x2b3145,
  monitor: 0x1c1f27,
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
  /** Mensajes entre agentes (ADR 0017): sobre, su etiqueta y la ruta entre oficinas. */
  envelope: 0xf4ead2,
  mailFill: 0xf5c542,
  mailInk: 0x0e1018,
  mailPath: 0xf5c542,
  /** Línea del orquestador a los agentes efímeros que lanzó (ADR 0019) y la de una tarea atascada (violeta: ni amarillo de mensajes ni rojo de error). */
  taskLine: 0x5ec8e5,
  taskStuck: 0xd36bff,
} as const;
