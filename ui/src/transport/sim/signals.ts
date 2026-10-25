// Mapeo mínimo señal normalizada → semáforo (ADR 0005 §5). Solo lo usa el simulador TS,
// que sustituye al core; se valida contra fixtures/sim/conformance/, igual que el reducer Rust.

import type { StatusKind, StatusReason } from "../../protocol";

export type Signal =
  | { type: "SessionStarted" }
  | { type: "PromptSubmitted" }
  | { type: "ToolStarted"; tool: string }
  | { type: "ToolFinished"; failed?: boolean }
  | { type: "AwaitingInput" }
  | { type: "Stopped" }
  | { type: "Error" }
  | { type: "SessionEnded" }
  /** Observación del escaneo de procesos: el proceso desapareció. */
  | { type: "ProcessGone" };

export type AgentStatus = {
  kind: StatusKind;
  reason: StatusReason;
  since: number;
  lastTool?: string;
  /** Hubo `SessionEnded` sin un `SessionStarted` posterior: si el proceso muere, no es un error. */
  exitExpected?: true;
};

function to(
  prev: AgentStatus | undefined,
  kind: StatusKind,
  reason: StatusReason,
  now: number,
  lastTool = prev?.lastTool,
  exitExpected: boolean | undefined = prev?.exitExpected,
): AgentStatus {
  const since = prev?.kind === kind ? prev.since : now;
  const next: AgentStatus = { kind, reason, since };
  if (lastTool !== undefined) next.lastTool = lastTool;
  if (exitExpected) next.exitExpected = true;
  return next;
}

export function applySignal(
  prev: AgentStatus | undefined,
  signal: Signal,
  now: number,
): AgentStatus | "removed" {
  if (prev?.kind === "dead") return prev;
  switch (signal.type) {
    case "ProcessGone":
      return prev?.exitExpected ? "removed" : to(prev, "dead", "processGone", now);
    case "SessionEnded":
      return to(prev, "idle", "hook", now, prev?.lastTool, true);
    case "SessionStarted":
      return to(prev, "idle", "hook", now, prev?.lastTool, false);
    case "Stopped":
      return to(prev, "idle", "hook", now);
    case "PromptSubmitted":
    case "ToolFinished": // un fallo de herramienta no es rojo
      return to(prev, "working", "hook", now);
    case "ToolStarted":
      return to(prev, "working", "hook", now, signal.tool);
    case "AwaitingInput":
      return to(prev, "waiting", "hook", now);
    case "Error":
      return to(prev, "error", "failure", now);
  }
}
