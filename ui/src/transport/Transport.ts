// Única puerta entre la UI y el backend (ADR 0003 §2). Sim en el navegador, Tauri en la app.
import type {
  AgentId,
  ConversationInfo,
  Hello,
  KillResult,
  Provider,
  ServerMsg,
  TerminalInfo,
  UiMetrics,
} from "../protocol";

/** Terminales de los agentes lanzados desde Kiti (ADR 0007). Solo existe en la app nativa. */
export interface TerminalApi {
  /** Abre el selector nativo de carpeta y lanza el CLI. `null` si el usuario cancela. */
  launch(provider: Provider): Promise<TerminalInfo | null>;
  list(): Promise<TerminalInfo[]>;
  /** Recibe primero el scrollback y después la salida en vivo. */
  attach(id: string, onData: (bytes: Uint8Array) => void): Promise<void>;
  input(id: string, data: string): Promise<void>;
  resize(id: string, cols: number, rows: number): Promise<void>;
}

/** Conversaciones cerradas de Claude que se pueden retomar (ADR 0009). Solo en la app nativa. */
export interface ConversationApi {
  /** Últimos 7 días, de la más reciente a la más vieja. Incluye las que siguen abiertas. */
  list(): Promise<ConversationInfo[]>;
  /** `claude --resume <id>` en su carpeta, dentro de una terminal de Kiti. */
  resume(sessionId: string): Promise<TerminalInfo>;
}

export interface Transport {
  /** "in-app": la UI muestra su AlertDialog (Sim). "native": confirma Rust con un diálogo nativo (Tauri). */
  readonly killConfirmation: "in-app" | "native";
  /** `undefined` en el simulador: no hay terminales que mostrar. */
  readonly terminals?: TerminalApi;
  readonly conversations?: ConversationApi;
  /** El primer mensaje es siempre `Snapshot`; después llegan `Diff` (~10 Hz). */
  connect(onMsg: (msg: ServerMsg) => void): Promise<Hello>;
  /** Termina el proceso del agente. Solo tras la confirmación que corresponda. */
  killAgent(id: AgentId): Promise<KillResult>;
  setDebugStream(on: boolean): Promise<void>;
  /** Resumen cada 2 s. No-op en Sim. */
  reportUiMetrics(m: UiMetrics): void;
  dispose(): void;
}
