// Parámetros de URL del modo sim y del ciclo visual (ADR 0003 §10).

export type Zoom = "far" | "mid" | "near";

export type Params = {
  sim: number;
  seed: number;
  /** Pasos virtuales de 100 ms antes del Snapshot. */
  steps: number;
  freeze: boolean;
  anim: boolean;
  zoom: Zoom;
  /** Id o índice de la oficina a enfocar. */
  focus?: string;
  bench?: string;
  /** Terminal falsa en el modo sim, para probar el panel de terminal (ADR 0007). */
  terminals: boolean;
};

const MAX_AGENTS = 200;

function int(value: string | null, fallback: number, min: number, max: number): number {
  if (value === null || !/^\d+$/.test(value)) return fallback;
  return Math.min(max, Math.max(min, Number(value)));
}

export function parseParams(search: string): Params {
  const q = new URLSearchParams(search);
  const zoom = q.get("zoom");
  const params: Params = {
    sim: int(q.get("sim"), 20, 0, MAX_AGENTS),
    seed: int(q.get("seed"), 1, 0, 2 ** 31 - 1),
    steps: int(q.get("steps"), 0, 0, 100_000),
    freeze: q.get("freeze") === "1",
    anim: q.get("anim") !== "0",
    zoom: zoom === "mid" || zoom === "near" ? zoom : "far",
    terminals: q.get("terminals") === "1",
  };
  const focus = q.get("focus");
  if (focus) params.focus = focus;
  const bench = q.get("bench");
  if (bench) params.bench = bench;
  return params;
}
