// Tablero de tareas de los agentes efímeros (ADR 0019).
import type { TaskInfo } from "../protocol";

export type TaskSummary = {
  /** `launching` + `running`. */
  active: number;
  /** `stuck` + `failed`. */
  attention: number;
  doneLast24h: number;
};

const DAY = 86_400_000;

/** Viva = sigue ocupando un agente (en curso o atascada). */
const isLive = (t: TaskInfo) =>
  t.state === "launching" || t.state === "running" || t.state === "stuck";

export function taskSummary(tasks: readonly TaskInfo[], now: number): TaskSummary {
  const out: TaskSummary = { active: 0, attention: 0, doneLast24h: 0 };
  for (const t of tasks) {
    if (t.state === "launching" || t.state === "running") out.active++;
    else if (t.state === "stuck" || t.state === "failed") out.attention++;
    else if (t.state === "done" && now - (t.finishedAt ?? t.createdAt) < DAY) out.doneLast24h++;
  }
  return out;
}

/** Primero las vivas y después las terminadas; dentro de cada grupo, de la más nueva a la más vieja. */
export function sortTasks(tasks: readonly TaskInfo[]): TaskInfo[] {
  return [...tasks].sort(
    (a, b) => Number(isLive(b)) - Number(isLive(a)) || b.createdAt - a.createdAt,
  );
}
