// Textos de estado y motivo compartidos por los paneles.
import type { StatusKind, StatusReason } from "../protocol";

export const STATUS_LABEL: Record<StatusKind, string> = {
  working: "trabajando",
  waiting: "esperando",
  idle: "inactivo",
  error: "error",
  dead: "caído",
};

export const REASON_LABEL: Record<StatusReason, string> = {
  hook: "hook del agente",
  sessionFile: "archivo de sesión de Claude",
  stale: "sin actividad reciente",
  failure: "error del agente",
  processGone: "el proceso terminó",
  startup: "detectado, aún sin eventos",
};

export function ago(ms: number): string {
  const sec = Math.max(0, Math.round(ms / 1000));
  if (sec < 60) return `hace ${sec} s`;
  const min = Math.round(sec / 60);
  return min < 60 ? `hace ${min} min` : `hace ${Math.round(min / 60)} h`;
}
