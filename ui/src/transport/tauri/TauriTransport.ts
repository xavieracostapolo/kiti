// Transport real para el shell Tauri (ADR 0003 §2, ADR 0004 §1).
// Se conecta al stream IPC mediante Channel<ServerMsg> y usa native dialogs para confirmación.

import { Channel, invoke } from "@tauri-apps/api/core";
import type { AgentId, Hello, KillResult, ServerMsg, UiMetrics } from "../../protocol";
import type { Transport } from "../Transport";

export class TauriTransport implements Transport {
  readonly killConfirmation = "native" as const;

  async connect(onMsg: (msg: ServerMsg) => void): Promise<Hello> {
    const channel = new Channel<ServerMsg>();
    channel.onmessage = (msg: ServerMsg) => {
      onMsg(msg);
    };
    return await invoke<Hello>("subscribe", { onMsg: channel });
  }

  async killAgent(id: AgentId): Promise<KillResult> {
    return await invoke<KillResult>("kill_agent", { id });
  }

  async setDebugStream(on: boolean): Promise<void> {
    await invoke<void>("set_debug_stream", { on });
  }

  reportUiMetrics(m: UiMetrics): void {
    void invoke<void>("report_ui_metrics", { m });
  }

  dispose(): void {}
}
