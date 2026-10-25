// Transport real para el shell Tauri (ADR 0003 §2, ADR 0004 §1).
// Se conecta al stream IPC mediante Channel<ServerMsg> y usa native dialogs para confirmación.

import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  AgentId,
  ConversationInfo,
  Hello,
  KillResult,
  Provider,
  ServerMsg,
  TerminalInfo,
  UiMetrics,
} from "../../protocol";
import type { ConversationApi, TerminalApi, Transport } from "../Transport";

const terminals: TerminalApi = {
  launch: (provider: Provider) => invoke<TerminalInfo | null>("launch_agent", { provider }),
  list: () => invoke<TerminalInfo[]>("list_terminals"),
  async attach(id, onData) {
    // La salida llega en binario (InvokeResponseBody::Raw): ArrayBuffer, no un array JSON.
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (data) => onData(new Uint8Array(data));
    await invoke("attach_terminal", { id, onData: channel });
  },
  input: (id, data) => invoke("terminal_input", { id, data }),
  resize: (id, cols, rows) => invoke("terminal_resize", { id, cols, rows }),
};

const conversations: ConversationApi = {
  list: () => invoke<ConversationInfo[]>("list_conversations"),
  resume: (sessionId) => invoke<TerminalInfo>("resume_conversation", { sessionId }),
};

export class TauriTransport implements Transport {
  readonly killConfirmation = "native" as const;
  readonly terminals = terminals;
  readonly conversations = conversations;

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
