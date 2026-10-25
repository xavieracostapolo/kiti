// Única puerta entre la UI y el backend (ADR 0003 §2). Sim en el navegador, Tauri en la app.
import type { AgentId, Hello, KillResult, ServerMsg, UiMetrics } from "../protocol";

export interface Transport {
  /** "in-app": la UI muestra su AlertDialog (Sim). "native": confirma Rust con un diálogo nativo (Tauri). */
  readonly killConfirmation: "in-app" | "native";
  /** El primer mensaje es siempre `Snapshot`; después llegan `Diff` (~10 Hz). */
  connect(onMsg: (msg: ServerMsg) => void): Promise<Hello>;
  /** Termina el proceso del agente. Solo tras la confirmación que corresponda. */
  killAgent(id: AgentId): Promise<KillResult>;
  setDebugStream(on: boolean): Promise<void>;
  /** Resumen cada 2 s. No-op en Sim. */
  reportUiMetrics(m: UiMetrics): void;
  dispose(): void;
}
