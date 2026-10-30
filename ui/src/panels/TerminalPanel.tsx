import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef } from "react";
import { useStore } from "zustand";
import { TERMINAL_FRACTION } from "../domain/layout";
import { displayName } from "../domain/names";
import type { KitiStore } from "../state/store";
import type { TerminalApi } from "../transport/Transport";
import { STATUS_COLOR } from "../world/palette";
import type { WorldHandle } from "../world/World";
import { STATUS_LABEL } from "./labels";
import s from "./panels.module.css";

const hex = (n: number) => `#${n.toString(16).padStart(6, "0")}`;
const WIDE_FRACTION = 0.62;

type Props = {
  store: KitiStore;
  api: TerminalApi;
  world?: () => WorldHandle | undefined;
};

/** Terminal real de un agente lanzado desde Kiti (ADR 0007), con xterm.js. */
export function TerminalPanel({ store, api, world }: Props) {
  const id = useStore(store, (st) => st.openTerminalId);
  const fraction = useStore(store, (st) => st.terminalFraction);
  const info = useStore(store, (st) =>
    st.openTerminalId
      ? [...st.terminals.values()].find((t) => t.id === st.openTerminalId)
      : undefined,
  );
  const agent = useStore(store, (st) =>
    info ? [...st.agents.agents.values()].find((a) => a.pid === info.pid) : undefined,
  );
  const git = useStore(store, (st) => (agent?.project ? st.git.get(agent.project) : undefined));
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = host.current;
    if (!id || !el) return;
    const term = new Terminal({
      cursorBlink: true,
      fontFamily: "Menlo, Monaco, monospace",
      fontSize: 13,
      theme: { background: "#14171f", foreground: "#f5f7fa" },
      scrollback: 5000,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    fit.fit();
    let active = true;
    let observer: ResizeObserver | undefined;
    const resize = () => {
      fit.fit();
      void api.resize(id, term.cols, term.rows);
    };
    const input = term.onData((data) => void api.input(id, data));
    void api
      .attach(id, (bytes) => term.write(bytes))
      .then(() => {
        if (!active) return;
        resize();
        observer = new ResizeObserver(resize);
        observer.observe(el);
      });
    term.focus();
    return () => {
      active = false;
      observer?.disconnect();
      input.dispose();
      term.dispose();
    };
  }, [id, api]);

  if (!id) return null;
  const folder = info ? displayName(info.cwd) : id;
  const title = agent?.title ?? folder;
  const metaParts = [
    ...(info ? [info.provider] : []),
    ...(agent?.title && info ? [folder] : []),
    ...(agent ? [STATUS_LABEL[agent.status.kind]] : []),
    ...(info && !info.running ? ["terminado"] : []),
  ];
  const subParts = [
    ...(git?.branch ? [git.branch] : []),
    ...(git && git.changed > 0 ? [`${git.changed} cambios`] : []),
    ...(agent?.model ? [agent.model] : []),
    ...(agent?.contextPct !== undefined ? [`${agent.contextPct} %`] : []),
    ...(agent?.costCents !== undefined ? [`$${(agent.costCents / 100).toFixed(2)}`] : []),
    ...(agent?.activity ? [agent.activity] : []),
  ];
  const isWide = fraction > TERMINAL_FRACTION + 0.05;

  const startDrag = (e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    const target = e.currentTarget;
    target.setPointerCapture(e.pointerId);
    const onMove = (ev: PointerEvent) => {
      const vw = Math.max(1, window.innerWidth);
      const rightEdge = vw - 12;
      const next = (rightEdge - ev.clientX) / vw;
      store.getState().setTerminalFraction(next);
    };
    const onUp = () => {
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onUp);
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onUp);
  };

  return (
    <section
      className={s.terminalPanel}
      style={{ width: `${Math.round(fraction * 1000) / 10}vw` }}
      aria-label="Terminal del agente"
    >
      <div
        className={s.terminalResizeHandle}
        title="Arrastrar para cambiar el ancho de la terminal"
        onPointerDown={startDrag}
      />
      <header className={s.terminalHeader}>
        <div className={s.terminalMeta}>
          <div className={s.terminalTitleRow}>
            {agent && (
              <span
                className={s.dot}
                style={{ background: hex(STATUS_COLOR[agent.status.kind]) }}
              />
            )}
            <strong className={s.terminalTitle}>{title}</strong>
            {metaParts.length > 0 && (
              <span className={s.terminalBadgeText}>· {metaParts.join(" · ")}</span>
            )}
          </div>
          {subParts.length > 0 && <div className={s.terminalSubRow}>{subParts.join(" · ")}</div>}
        </div>
        <div className={s.terminalActions}>
          <button
            type="button"
            className={s.button}
            title={isWide ? "Ancho estándar" : "Ampliar terminal"}
            onClick={() =>
              store.getState().setTerminalFraction(isWide ? TERMINAL_FRACTION : WIDE_FRACTION)
            }
          >
            {isWide ? "⇥" : "⇤"}
          </button>
          {agent && (
            <button type="button" className={s.button} onClick={() => world?.()?.focus(agent.id)}>
              Enfocar
            </button>
          )}
          {agent && (
            <button
              type="button"
              className={`${s.button} ${s.danger}`}
              title="Cerrar oficina y terminar el proceso"
              onClick={() => store.getState().requestKill(agent.id)}
            >
              Terminar
            </button>
          )}
          <button
            type="button"
            className={s.button}
            onClick={() => store.getState().closeTerminal()}
          >
            Cerrar
          </button>
        </div>
      </header>
      <div ref={host} className={s.terminalHost} />
    </section>
  );
}
