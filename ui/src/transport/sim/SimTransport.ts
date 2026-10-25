// Transport simulado: sustituye al core en el navegador y en CI (ADR 0003 §2).

import type { AgentId, AgentState, Hello, KillResult, ServerMsg, UiMetrics } from "../../protocol";
import { PROTOCOL_VERSION } from "../../protocol";
import type { Transport } from "../Transport";
import { SimFleet, TICK_MS } from "./fleet";

export type Schedule = (tick: () => void) => () => void;

export type SimOptions = {
  count: number;
  seed: number;
  /** Pasos virtuales que se avanzan antes del Snapshot. */
  steps?: number;
  /** Sin pasos automáticos: el mundo queda quieto (capturas, test de reposo). */
  freeze?: boolean;
  schedule?: Schedule;
};

const intervalSchedule: Schedule = (tick) => {
  const handle = setInterval(tick, TICK_MS);
  return () => clearInterval(handle);
};

export class SimTransport implements Transport {
  readonly killConfirmation = "in-app" as const;
  private readonly fleet: SimFleet;
  private readonly opts: SimOptions;
  private onMsg: ((msg: ServerMsg) => void) | undefined;
  private cancel: (() => void) | undefined;
  private seq = 0;

  constructor(opts: SimOptions) {
    this.opts = opts;
    this.fleet = new SimFleet({ count: opts.count, seed: opts.seed });
  }

  async connect(onMsg: (msg: ServerMsg) => void): Promise<Hello> {
    this.onMsg = onMsg;
    for (let i = 0; i < (this.opts.steps ?? 0); i++) this.fleet.step();
    this.seq = 0;
    onMsg({ type: "Snapshot", seq: this.seq, agents: this.fleet.snapshot() });
    if (!this.opts.freeze)
      this.cancel = (this.opts.schedule ?? intervalSchedule)(() => this.tick());
    return { protocolVersion: PROTOCOL_VERSION, appVersion: "0.1.0-sim", source: "sim" };
  }

  /** Reloj virtual del simulador en epoch ms (para "hace N s" en los paneles). */
  now(): number {
    return this.fleet.now;
  }

  /** Avanza `n` pasos a mano (window.__kiti.step). */
  advance(n: number): void {
    for (let i = 0; i < n; i++) this.tick();
  }

  private tick(): void {
    const { upserts, removed } = this.fleet.step();
    this.emitDiff(upserts, removed);
  }

  private emitDiff(upserts: AgentState[], removed: AgentId[]): void {
    if (upserts.length === 0 && removed.length === 0) return;
    this.seq += 1;
    this.onMsg?.({ type: "Diff", seq: this.seq, ingestMs: this.fleet.now, upserts, removed });
  }

  async killAgent(id: AgentId): Promise<KillResult> {
    if (!this.fleet.kill(id)) return { kind: "notFound" };
    if (this.opts.freeze) this.emitDiff([], this.fleet.flushKills());
    return { kind: "killed" };
  }

  async setDebugStream(_on: boolean): Promise<void> {}

  reportUiMetrics(_m: UiMetrics): void {}

  dispose(): void {
    this.cancel?.();
    this.cancel = undefined;
    this.onMsg = undefined;
  }
}
