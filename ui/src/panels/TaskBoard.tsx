import { useStore } from "zustand";
import type { TaskInfo } from "../protocol";
import type { KitiStore } from "../state/store";
import { groupTasks, taskSummary } from "../state/tasks";
import { ago } from "./labels";
import s from "./panels.module.css";

const STATE_LABEL: Record<TaskInfo["state"], string> = {
  launching: "lanzando",
  running: "en curso",
  stuck: "atascada",
  failed: "fallida",
  done: "hecha",
};

/** Chip de la barra superior: abre el tablero; ámbar si alguna tarea está atascada o falló. */
export function TaskChip({ store }: { store: KitiStore }) {
  const tasks = useStore(store, (st) => st.tasks);
  const open = useStore(store, (st) => st.sidePanel === "tasks");
  if (tasks.length === 0) return null;
  // El resumen solo mira los estados abiertos; la hora no cuenta aquí.
  const { active, attention } = taskSummary(tasks, 0);
  const n = active + attention;
  const label = `${n} ${n === 1 ? "tarea" : "tareas"}`;
  return (
    <button
      type="button"
      className={`${s.mailChip} ${attention > 0 ? s.mailChipAlert : ""}`}
      data-alert={attention > 0 ? "true" : undefined}
      aria-label={`Tablero de tareas: ${label}${attention > 0 ? ", con atención" : ""}`}
      aria-pressed={open}
      title="Tareas de los agentes efímeros"
      onClick={() => {
        const st = store.getState();
        if (open) st.closeSidePanel();
        else st.openSidePanel("tasks");
      }}
    >
      ⚙ {label}
    </button>
  );
}

/** Tablero de las tareas de los agentes efímeros (ADR 0019). */
export function TaskBoard({ store, now }: { store: KitiStore; now: () => number }) {
  const tasks = useStore(store, (st) => st.tasks);
  const open = useStore(store, (st) => st.sidePanel === "tasks");
  if (!open) return null;
  const t = now();
  const { active, attention, doneLast24h } = taskSummary(tasks, t);
  return (
    <aside className={s.mailPanel} aria-label="Tablero de tareas">
      <div className={s.mailHead}>
        <h2 className={s.mailTitle}>Tareas</h2>
        <button
          type="button"
          className={s.closeBtn}
          aria-label="Cerrar tareas"
          onClick={() => store.getState().closeSidePanel()}
        >
          ✕
        </button>
      </div>
      <div className={s.taskGroups}>
        {groupTasks(tasks).map((g) => (
          <section key={g.orchestrator} aria-label={`Lanzadas por ${g.orchestrator}`}>
            <h3 className={s.taskGroupHead}>
              lanzadas por {g.orchestrator} · {g.active} en curso
            </h3>
            <ul className={s.taskList} aria-label={`Tareas de ${g.orchestrator}`}>
              {g.tasks.map((task) => (
                <TaskRow key={task.id} task={task} now={t} />
              ))}
            </ul>
          </section>
        ))}
      </div>
      <div className={s.mailFoot}>
        <span>
          {active} en curso · {attention} con atención
        </span>
        <span>hechas en 24 h: {doneLast24h}</span>
      </div>
    </aside>
  );
}

function TaskRow({ task, now }: { task: TaskInfo; now: number }) {
  const finished = task.state === "done" || task.state === "failed";
  const when = finished
    ? `terminó ${ago(now - (task.finishedAt ?? task.createdAt))}`
    : ago(now - task.createdAt);
  const name = task.alias ?? `tarea ${task.id}`;
  return (
    <li
      className={s.taskRow}
      data-state={task.state}
      aria-label={`${name}, ${STATE_LABEL[task.state]}`}
    >
      <div className={s.taskTop}>
        <strong>{name}</strong>
        <span className={s.taskState}>{STATE_LABEL[task.state]}</span>
      </div>
      <div className={s.taskMeta}>
        {task.role} · <code>{task.branch}</code> · {task.model}
      </div>
      <div className={s.taskMeta}>
        {when}
        {task.pr && (
          <>
            {" · "}
            <a href={task.pr} target="_blank" rel="noreferrer">
              PR #{task.pr.split("/").pop()}
            </a>
          </>
        )}
      </div>
      {task.reason && <div className={s.taskReason}>{task.reason}</div>}
    </li>
  );
}
