import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef } from "react";
import { useStore } from "zustand";
import { displayName } from "../domain/names";
import type { KitiStore } from "../state/store";
import type { TerminalApi } from "../transport/Transport";
import { STATUS_COLOR } from "../world/palette";
import { STATUS_LABEL } from "./labels";
import s from "./panels.module.css";

const hex = (n: number) => `#${n.toString(16).padStart(6, "0")}`;

type Props = { store: KitiStore; api: TerminalApi };

/** Terminal real de un agente lanzado desde Kiti (ADR 0007), con xterm.js. */
export function TerminalPanel({ store, api }: Props) {
  const id = useStore(store, (st) => st.openTerminalId);
  const info = useStore(store, (st) =>
    st.openTerminalId
      ? [...st.terminals.values()].find((t) => t.id === st.openTerminalId)
      : undefined,
  );
  const agent = useStore(store, (st) =>
    info ? [...st.agents.agents.values()].find((a) => a.pid === info.pid) : undefined,
  );
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
  return (
    <section className={s.terminalPanel} aria-label="Terminal del agente">
      <header className={s.terminalHeader}>
        <span>
          {agent && (
            <span className={s.dot} style={{ background: hex(STATUS_COLOR[agent.status.kind]) }} />
          )}{" "}
          {info ? `${info.provider} · ${displayName(info.cwd)}` : id}
          {agent ? ` · ${STATUS_LABEL[agent.status.kind]}` : ""}
          {info && !info.running ? " · terminado" : ""}
        </span>
        <button type="button" className={s.button} onClick={() => store.getState().closeTerminal()}>
          Cerrar
        </button>
      </header>
      <div ref={host} className={s.terminalHost} />
    </section>
  );
}
