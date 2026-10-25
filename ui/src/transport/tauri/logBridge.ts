// Reenvía console.info/warn/error y los errores no capturados del WebView al log del core
// (comando `ui_log`). En Tauri no hay otra forma de leer la consola sin abrir el inspector.
import { invoke } from "@tauri-apps/api/core";

type Level = "info" | "warn" | "error";

function one(a: unknown): string {
  if (a instanceof Error) return `${a.message}\n${a.stack ?? ""}`;
  // Los errores de los commands llegan como objetos ({ code, message }): sin esto, "[object Object]".
  if (typeof a === "object" && a !== null) {
    try {
      return JSON.stringify(a);
    } catch {
      return String(a);
    }
  }
  return String(a);
}

const text = (args: unknown[]) => args.map(one).join(" ");

export function installLogBridge(
  send = (level: Level, message: string) => {
    void invoke("ui_log", { level, message }).catch(() => {});
  },
): void {
  for (const level of ["info", "warn", "error"] as const) {
    const original = console[level].bind(console);
    console[level] = (...args: unknown[]) => {
      original(...args);
      send(level, text(args));
    };
  }
  window.addEventListener("error", (e) =>
    send("error", `${e.message} @ ${e.filename}:${e.lineno}`),
  );
  window.addEventListener("unhandledrejection", (e) =>
    send("error", `unhandled: ${text([e.reason])}`),
  );
}
