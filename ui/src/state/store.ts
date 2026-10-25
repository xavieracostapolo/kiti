// Store único (zustand vanilla). El mundo Pixi se suscribe fuera de React; los paneles usan
// selectores finos. El estado de vista vive en un slice aparte (ADR 0003 §4).

import { createStore } from "zustand/vanilla";
import type { Lod } from "../domain/lod";
import type { AgentId, CoreMetrics, ServerMsg, Source, TerminalInfo } from "../protocol";
import { type AgentsState, applyMsg, EMPTY_AGENTS } from "./agents";

export type ViewState = {
  selectedId?: AgentId;
  pendingKill?: AgentId;
  lod: Lod;
};

/** Oficina bajo el ratón, en px de pantalla, para el tooltip. */
export type Hover = { id: AgentId; x: number; y: number };

export type KitiState = {
  agents: AgentsState;
  /** Terminales de los agentes lanzados desde Kiti, por pid (ADR 0007). */
  terminals: ReadonlyMap<number, TerminalInfo>;
  openTerminalId: string | undefined;
  hover: Hover | undefined;
  setTerminals(list: readonly TerminalInfo[]): void;
  terminalFor(pid: number): TerminalInfo | undefined;
  openTerminal(id: string): void;
  closeTerminal(): void;
  setHover(hover: Hover | undefined): void;
  view: ViewState;
  /** De dónde vienen los agentes (Hello.source). `undefined` hasta conectar. */
  source?: Source;
  /** Último `Metrics` del core; solo llega con el stream de debug activo. */
  coreMetrics?: CoreMetrics;
  setSource(source: Source): void;
  receive(msg: ServerMsg): void;
  select(id: AgentId | undefined): void;
  requestKill(id: AgentId): void;
  cancelKill(): void;
  setLod(lod: Lod): void;
};

export type KitiStore = ReturnType<typeof createKitiStore>;

function without<K extends keyof ViewState>(view: ViewState, key: K): ViewState {
  const { [key]: _, ...rest } = view;
  return rest as ViewState;
}

export function createKitiStore() {
  return createStore<KitiState>()((set, get) => ({
    agents: EMPTY_AGENTS,
    terminals: new Map(),
    openTerminalId: undefined,
    hover: undefined,
    view: { lod: "far" },
    setTerminals(list) {
      set({ terminals: new Map(list.map((t) => [t.pid, t])) });
    },
    terminalFor(pid) {
      return get().terminals.get(pid);
    },
    openTerminal(id) {
      if (get().openTerminalId !== id) set({ openTerminalId: id });
    },
    closeTerminal() {
      if (get().openTerminalId !== undefined) set({ openTerminalId: undefined });
    },
    setHover(hover) {
      const prev = get().hover;
      // Solo cambia la oficina, no cada píxel: el tooltip se ancla a la posición de entrada.
      if (prev?.id === hover?.id) return;
      set({ hover });
    },
    setSource(source) {
      set({ source });
    },
    receive(msg) {
      if (msg.type === "Metrics") {
        const { type: _, ...coreMetrics } = msg;
        set({ coreMetrics });
        return;
      }
      const agents = applyMsg(get().agents, msg);
      if (agents === get().agents) return;
      let view = get().view;
      if (view.selectedId !== undefined && !agents.agents.has(view.selectedId)) {
        view = without(view, "selectedId");
      }
      if (view.pendingKill !== undefined && !agents.agents.has(view.pendingKill)) {
        view = without(view, "pendingKill");
      }
      set({ agents, view });
    },
    select(id) {
      const view = get().view;
      set({ view: id === undefined ? without(view, "selectedId") : { ...view, selectedId: id } });
    },
    requestKill(id) {
      set({ view: { ...get().view, pendingKill: id } });
    },
    cancelKill() {
      set({ view: without(get().view, "pendingKill") });
    },
    setLod(lod) {
      if (get().view.lod !== lod) set({ view: { ...get().view, lod } });
    },
  }));
}
