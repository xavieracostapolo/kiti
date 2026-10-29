// Store único (zustand vanilla). El mundo Pixi se suscribe fuera de React; los paneles usan
// selectores finos. El estado de vista vive en un slice aparte (ADR 0003 §4).

import { createStore } from "zustand/vanilla";
import type { Lod } from "../domain/lod";
import type { LayoutMode } from "../domain/zones";
import type {
  AgentId,
  ConversationInfo,
  CoreMetrics,
  GitInfo,
  Provider,
  QuotaWindow,
  ServerMsg,
  Source,
  TerminalInfo,
} from "../protocol";
import { type AgentsState, applyMsg, EMPTY_AGENTS } from "./agents";
import { EMPTY_FILTER, type FilterKind, type OfficeFilter, toggleKind } from "./filter";
import { dormantRooms } from "./rooms";

export type ViewState = {
  /** Oficina seleccionada: un agente o una conversación cerrada (`conv:<id>`). */
  selectedId?: AgentId;
  pendingKill?: AgentId;
  lod: Lod;
};

/** Oficina bajo el ratón, en px de pantalla, para el tooltip. */
export type Hover = { id: AgentId; x: number; y: number };

export type KitiState = {
  agents: AgentsState;
  /** Estado de git de cada proyecto (ADR 0016); `null` si no es un repositorio. */
  git: ReadonlyMap<string, GitInfo | null>;
  /** Última cuota de uso de cada proveedor (ADR 0015). */
  quotas: ReadonlyMap<Provider, readonly QuotaWindow[]>;
  /** Conversaciones de los últimos 7 días tal como llegan del core (ADR 0009). */
  conversations: readonly ConversationInfo[];
  /** Oficinas apagadas: las conversaciones sin agente encendido, por id de oficina. */
  dormant: ReadonlyMap<string, ConversationInfo>;
  setConversations(list: readonly ConversationInfo[]): void;
  /** Filtro de oficinas: las que no coinciden se atenúan (Hito 13). */
  filter: OfficeFilter;
  toggleFilterKind(kind: FilterKind): void;
  setFilterText(text: string): void;
  clearFilter(): void;
  /** Modo de distribución del mapa de zonas (`dynamic`, `horizontal`, `vertical`). */
  layoutMode: LayoutMode;
  setLayoutMode(mode: LayoutMode): void;
  /** Terminales de los agentes lanzados desde Kiti, por pid (ADR 0007). */
  terminals: ReadonlyMap<number, TerminalInfo>;
  openTerminalId: string | undefined;
  /** PID de un agente recién lanzado: al aparecer su oficina se enfoca y se selecciona. */
  pendingLaunchPid: number | undefined;
  hover: Hover | undefined;
  setPendingLaunch(pid: number | undefined): void;
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

/** ¿Sigue existiendo esa oficina, encendida o apagada? */
function exists(id: string, agents: AgentsState, dormant: ReadonlyMap<string, unknown>) {
  return agents.agents.has(id) || dormant.has(id);
}

export function createKitiStore() {
  return createStore<KitiState>()((set, get) => ({
    agents: EMPTY_AGENTS,
    quotas: new Map(),
    git: new Map(),
    conversations: [],
    dormant: new Map(),
    filter: EMPTY_FILTER,
    toggleFilterKind(kind) {
      set({ filter: toggleKind(get().filter, kind) });
    },
    setFilterText(text) {
      if (get().filter.text !== text) set({ filter: { ...get().filter, text } });
    },
    clearFilter() {
      if (get().filter !== EMPTY_FILTER) set({ filter: EMPTY_FILTER });
    },
    layoutMode: "dynamic",
    setLayoutMode(layoutMode) {
      if (get().layoutMode !== layoutMode) set({ layoutMode });
    },
    setConversations(list) {
      const { agents, dormant: prev, view } = get();
      const dormant = dormantRooms(agents.agents, list, prev);
      const selected = view.selectedId;
      set({
        conversations: list,
        dormant,
        view: selected && !exists(selected, agents, dormant) ? without(view, "selectedId") : view,
      });
    },
    terminals: new Map(),
    openTerminalId: undefined,
    pendingLaunchPid: undefined,
    hover: undefined,
    setPendingLaunch(pid) {
      set({ pendingLaunchPid: pid });
    },
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
      if (msg.type === "Git") {
        const git = new Map(get().git);
        git.set(msg.project, msg.git ?? null);
        set({ git });
        return;
      }
      if (msg.type === "Quota") {
        const quotas = new Map(get().quotas);
        quotas.set(msg.provider, msg.windows);
        set({ quotas });
        return;
      }
      if (msg.type === "Focus") {
        // Clic en un aviso: como un agente recién lanzado, el mundo lo enfoca y lo selecciona.
        const agent = get().agents.agents.get(msg.id);
        if (agent) set({ pendingLaunchPid: agent.pid, openTerminalId: undefined });
        return;
      }
      const agents = applyMsg(get().agents, msg);
      if (agents === get().agents) return;
      const dormant = dormantRooms(agents.agents, get().conversations, get().dormant);
      let view = get().view;
      if (view.selectedId !== undefined && !exists(view.selectedId, agents, dormant)) {
        view = without(view, "selectedId");
      }
      if (view.pendingKill !== undefined && !agents.agents.has(view.pendingKill)) {
        view = without(view, "pendingKill");
      }
      set({ agents, dormant, view });
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
