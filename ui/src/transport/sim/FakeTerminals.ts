// Terminales falsas para el modo sim (`?terminals=1`): permiten probar en el navegador y en E2E
// el panel de terminal y "Nuevo agente" sin Tauri ni PTY. Repiten lo que se escribe, como `cat`.
import type { Provider, TerminalInfo } from "../../protocol";
import type { TerminalApi } from "../Transport";

const encoder = new TextEncoder();

type Target = { pid: number; provider: Provider; cwd: string };

export class FakeTerminals implements TerminalApi {
  private readonly sinks = new Map<string, (bytes: Uint8Array) => void>();
  private readonly open: TerminalInfo[] = [];

  /**
   * `attached`: agente que ya tiene terminal al arrancar. `launchable`: agente que "aparece" con
   * terminal al usar "Nuevo agente" (en el simulador no se crea un proceso nuevo).
   */
  constructor(
    attached: Target,
    private readonly launchable?: Target,
  ) {
    this.open.push(this.info("sim-1", attached));
  }

  private info(id: string, { pid, provider, cwd }: Target): TerminalInfo {
    return { id, pid, provider, cwd, running: true };
  }

  async launch(): Promise<TerminalInfo | null> {
    if (!this.launchable || this.open.some((t) => t.id === "sim-2")) return null;
    const info = this.info("sim-2", this.launchable);
    this.open.push(info);
    return info;
  }

  async list(): Promise<TerminalInfo[]> {
    return [...this.open];
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
