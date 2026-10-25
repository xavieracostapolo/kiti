// Terminal falsa para el modo sim (`?terminals=1`): permite probar el panel de terminal en el
// navegador y en E2E sin Tauri ni PTY. Repite lo que se escribe, como un `cat`.
import type { Provider, TerminalInfo } from "../../protocol";
import type { TerminalApi } from "../Transport";

const encoder = new TextEncoder();

export class FakeTerminals implements TerminalApi {
  private readonly sinks = new Map<string, (bytes: Uint8Array) => void>();

  constructor(private readonly attached: { pid: number; provider: Provider; cwd: string }) {}

  async launch(): Promise<TerminalInfo | null> {
    return null;
  }

  async list(): Promise<TerminalInfo[]> {
    const { pid, provider, cwd } = this.attached;
    return [{ id: "sim-1", pid, provider, cwd, running: true }];
  }

  async attach(id: string, onData: (bytes: Uint8Array) => void): Promise<void> {
    this.sinks.set(id, onData);
    onData(encoder.encode("Terminal simulada de Kiti\r\n$ "));
  }

  async input(id: string, data: string): Promise<void> {
    this.sinks.get(id)?.(encoder.encode(data === "\r" ? "\r\n$ " : data));
  }

  async resize(): Promise<void> {}
}
