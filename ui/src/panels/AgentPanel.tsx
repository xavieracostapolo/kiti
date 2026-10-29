import { useState } from "react";
import { useStore } from "zustand";
import { displayName } from "../domain/names";
import { projectName } from "../domain/zones";
import type { AgentId, FocusResult } from "../protocol";
import type { KitiStore } from "../state/store";
import { STATUS_COLOR } from "../world/palette";
import type { WorldHandle } from "../world/World";
import { ago, REASON_LABEL, STATUS_LABEL } from "./labels";
import s from "./panels.module.css";

const hex = (n: number) => `#${n.toString(16).padStart(6, "0")}`;

type Props = {
  store: KitiStore;
  world: () => WorldHandle | undefined;
  now: () => number;
  /** Hay terminales (app nativa): se ofrece abrir la del agente o se explica por qué no. */
  terminalsAvailable?: boolean;
  /** App nativa: lleva a la terminal de un agente abierto fuera de Kiti (ADR 0012). */
  onFocusExternal?: (id: AgentId) => Promise<FocusResult>;
};

const FOCUS_TEXT = (r: FocusResult): string => {
  switch (r.kind) {
    case "focused":
      return `Abierto en ${r.app}: ya tienes su pestaña al frente.`;
    case "appOnly":
      return `Abierto en ${r.app}: está al frente, pero no supe cuál es su pestaña.`;
    case "unknown":
      return "No sé en qué terminal está (¿tmux propio o ssh?).";
    case "notFound":
      return "Este agente ya no existe.";
  }
};

/** Detalle del agente seleccionado. Solo se re-renderiza cuando cambia ese agente. */
export function AgentPanel(props: Props) {
  // El resultado de "Ir a su terminal" es de cada agente: al cambiar de oficina, empieza vacío.
  const selected = useStore(props.store, (st) => st.view.selectedId);
  return <Details key={selected ?? ""} {...props} />;
}

function Details({ store, world, now, terminalsAvailable = false, onFocusExternal }: Props) {
  const [focus, setFocus] = useState<FocusResult | undefined>();
  const agent = useStore(store, (st) =>
    st.view.selectedId ? st.agents.agents.get(st.view.selectedId) : undefined,
  );
  const git = useStore(store, (st) => (agent?.project ? st.git.get(agent.project) : undefined));
  const terminal = useStore(store, (st) => (agent ? st.terminals.get(agent.pid) : undefined));
  // Con la terminal abierta, su cabecera ya dice qué agente es: el panel no tapa oficinas.
  const terminalOpen = useStore(store, (st) => st.openTerminalId !== undefined);
  if (!agent || terminalOpen) return null;
  const gitText = git
    ? [
        git.branch ?? "HEAD separado",
        ...(git.changed > 0 ? [`${git.changed} cambios`] : []),
        ...(git.ahead > 0 ? [`↑${git.ahead}`] : []),
        ...(git.behind > 0 ? [`↓${git.behind}`] : []),
      ].join(" · ")
    : undefined;
  return (
    <aside className={s.agentPanel} aria-label="Agente seleccionado">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
        <h2 className={s.agentTitle} style={{ margin: 0 }}>
          <span className={s.dot} style={{ background: hex(STATUS_COLOR[agent.status.kind]) }} />
          {agent.title ?? displayName(agent.cwd)}
        </h2>
        <button
          type="button"
          className={s.closeBtn}
          title="Cerrar panel"
          onClick={() => store.getState().select(undefined)}
        >
          ✕
        </button>
      </div>
      <dl className={s.fields}>
        {agent.project && agent.project !== agent.cwd && (
          <>
            <dt>proyecto</dt>
            <dd>{projectName(agent.project)}</dd>
          </>
        )}
        {gitText && (
          <>
            <dt>git</dt>
            <dd>{gitText}</dd>
          </>
        )}
        {agent.activity && (
          <>
            <dt>ahora</dt>
            <dd>{agent.activity}</dd>
          </>
        )}
        {agent.lastPrompt && (
          <>
            <dt>último pedido</dt>
            <dd>{agent.lastPrompt}</dd>
          </>
        )}
        {agent.model && (
          <>
            <dt>modelo</dt>
            <dd>{agent.model}</dd>
          </>
        )}
        {agent.contextPct !== undefined && (
          <>
            <dt>contexto</dt>
            <dd>{agent.contextPct} %</dd>
          </>
        )}
        {agent.costCents !== undefined && (
          <>
            <dt>coste</dt>
            <dd>${(agent.costCents / 100).toFixed(2)} sesión</dd>
          </>
        )}
        <dt>proveedor</dt>
        <dd>{agent.provider}</dd>
        <dt>estado</dt>
        <dd>
          {STATUS_LABEL[agent.status.kind]} · {ago(now() - agent.status.since)}
        </dd>
        <dt>motivo</dt>
        <dd>{REASON_LABEL[agent.status.reason]}</dd>
        <dt>herramienta</dt>
        <dd>{agent.lastTool ?? "—"}</dd>
        <dt>último evento</dt>
        <dd>{ago(now() - agent.lastEventAt)}</dd>
        <dt>pid</dt>
        <dd>{agent.pid}</dd>
        <dt>cwd</dt>
        <dd>{agent.cwd}</dd>
      </dl>
      {terminalsAvailable && !terminal && (
        <p className={s.notice} role="status">
          {focus
            ? FOCUS_TEXT(focus)
            : "Abierto fuera de Kiti: su terminal está en otra app. «Ir a su terminal» te lleva a su pestaña."}
        </p>
      )}
      <div className={s.panelActions}>
        {terminal && (
          <button
            type="button"
            className={s.button}
            onClick={() => store.getState().openTerminal(terminal.id)}
          >
            Abrir terminal
          </button>
        )}
        {onFocusExternal && !terminal && (
          <button
            type="button"
            className={s.button}
            onClick={() => void onFocusExternal(agent.id).then(setFocus)}
          >
            Ir a su terminal
          </button>
        )}
        <button type="button" className={s.button} onClick={() => world()?.focus(agent.id)}>
          Enfocar
        </button>
        <button
          type="button"
          className={`${s.button} ${s.danger}`}
          onClick={() => store.getState().requestKill(agent.id)}
        >
          Cerrar oficina
        </button>
      </div>
    </aside>
  );
}
