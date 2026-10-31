// window.__kiti: ganchos para capturas, tests E2E y benchmark (solo modo sim, ADR 0003 §10).

import { type Counts, countsByKind } from "../state/agents";
import type { KitiStore } from "../state/store";
import { type TaskSummary, taskSummary } from "../state/tasks";
import type { Metrics, MetricsSnapshot } from "../telemetry/metrics";
import type { BenchResult, WorldHandle } from "../world/World";
import type { Zoom } from "./params";

export type KitiDev = {
  ready: boolean;
  step(n?: number): void;
  focus(idOrIndex: string): void;
  setZoom(zoom: Zoom): void;
  fitAll(): void;
  metrics(): MetricsSnapshot;
  counts(): Counts;
  ids(): string[];
  frames(): number;
  bench(durationMs?: number): Promise<BenchResult>;
  kill(idOrIndex: string): void;
  screenCenter(idOrIndex: string): { x: number; y: number } | undefined;
  scale(): number;
  lod(): string;
  /** Zonas por proyecto con su número de oficinas (ADR 0009). */
  zones(): { name: string; count: number }[];
  /** Ids de las oficinas apagadas (conversaciones cerradas). */
  dormant(): string[];
  /** Ids de las oficinas atenuadas por el filtro. */
  dimmed(): string[];
  /** Texto del tablero de una oficina si se ve (solo en el zoom cercano). */
  board(id: string): string | null;
  /** Alias y sobre de una oficina (ADR 0017). */
  mail(id: string): string | null;
  /** Sobres dibujados entre oficinas (ADR 0017). */
  envelopes(): number;
  /** Líneas del orquestador a sus agentes efímeros (ADR 0019). */
  taskLinks(): number;
  /** Etiqueta de agentes efímeros de una oficina (ADR 0019). */
  taskTag(id: string): string | null;
  /** Recuerdos de proyecto cargados (ADR 0018). */
  memory(): number;
  /** Resumen del tablero de tareas (ADR 0019). */
  tasks(): TaskSummary;
};

declare global {
  interface Window {
    __kiti?: KitiDev;
  }
}

export function installDevHooks(
  store: KitiStore,
  sim: { advance(n: number): void; now?(): number } | undefined,
  metrics: Metrics,
  getWorld: () => WorldHandle | undefined,
): KitiDev {
  const ids = () => [...store.getState().agents.agents.keys()];
  const dev: KitiDev = {
    ready: false,
    step: (n = 1) => sim?.advance(n),
    focus: (id) => getWorld()?.focus(id),
    setZoom: (z) => getWorld()?.setZoom(z),
    fitAll: () => getWorld()?.fitAll(),
    metrics: () => metrics.snapshot(),
    counts: () => countsByKind(store.getState().agents.agents),
    ids,
    frames: () => getWorld()?.frames() ?? 0,
    bench: async (ms) => {
      const w = getWorld();
      if (!w) throw new Error("world not ready");
      return w.bench(ms);
    },
    screenCenter: (id) => getWorld()?.screenCenter(id),
    scale: () => getWorld()?.scale() ?? 0,
    lod: () => store.getState().view.lod,
    zones: () => getWorld()?.zones() ?? [],
    dormant: () => [...store.getState().dormant.keys()],
    dimmed: () => getWorld()?.dimmed() ?? [],
    board: (id) => getWorld()?.board(id) ?? null,
    mail: (id) => getWorld()?.mail(id) ?? null,
    envelopes: () => getWorld()?.envelopes() ?? 0,
    taskLinks: () => getWorld()?.taskLinks() ?? 0,
    taskTag: (id) => getWorld()?.taskTag(id) ?? null,
    memory: () => store.getState().memories.length,
    tasks: () => taskSummary(store.getState().tasks, sim?.now?.() ?? Date.now()),
    kill: (idOrIndex) => {
      const id = /^\d+$/.test(idOrIndex) ? ids()[Number(idOrIndex)] : idOrIndex;
      if (id) store.getState().requestKill(id);
    },
  };
  window.__kiti = dev;
  return dev;
}
