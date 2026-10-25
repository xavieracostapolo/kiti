// @vitest-environment jsdom
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { beforeEach, describe, expect, it } from "vitest";
import type { ServerMsg } from "../../protocol";
import { TauriTransport } from "./TauriTransport";

describe("TauriTransport", () => {
  beforeEach(() => {
    clearMocks();
  });

  it("declara killConfirmation nativa para evitar diálogos duplicados", () => {
    const t = new TauriTransport();
    expect(t.killConfirmation).toBe("native");
  });

  it("connect invoca subscribe y recibe Hello", async () => {
    const received: ServerMsg[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "subscribe") {
        expect(args).toHaveProperty("onMsg");
        return {
          protocolVersion: 1,
          appVersion: "0.1.0",
          source: "sim",
        };
      }
    });

    const t = new TauriTransport();
    const hello = await t.connect((msg) => received.push(msg));
    expect(hello.protocolVersion).toBe(1);
    expect(hello.appVersion).toBe("0.1.0");
    expect(hello.source).toBe("sim");
  });

  it("killAgent invoca el comando con el id", async () => {
    mockIPC((cmd, args) => {
      if (cmd === "kill_agent") {
        expect(args).toEqual({ id: "claude:100:200" });
        return { kind: "killed" };
      }
    });

    const t = new TauriTransport();
    const res = await t.killAgent("claude:100:200");
    expect(res).toEqual({ kind: "killed" });
  });

  it("setDebugStream y reportUiMetrics invocan los comandos respectivos", async () => {
    let debugCalled = false;
    let metricsCalled = false;

    mockIPC((cmd, args) => {
      if (cmd === "set_debug_stream") {
        debugCalled = true;
        expect(args).toEqual({ on: true });
        return;
      }
      if (cmd === "report_ui_metrics") {
        metricsCalled = true;
        return;
      }
    });

    const t = new TauriTransport();
    await t.setDebugStream(true);
    expect(debugCalled).toBe(true);

    t.reportUiMetrics({
      frameMsP50: 8,
      frameMsP95: 14,
      diffToPaintMsP50: 10,
      diffToPaintMsP95: 25,
      fps: 60,
      idleFrames: 0,
      officesVisible: 20,
    });
    expect(metricsCalled).toBe(true);
  });
});

describe("TauriTransport.terminals", () => {
  beforeEach(() => clearMocks());

  it("lanza un agente pidiendo solo el proveedor", async () => {
    const info = { id: "pty-1", pid: 9, provider: "claude", cwd: "/w", running: true };
    mockIPC((cmd, args) => {
      if (cmd === "launch_agent") {
        expect(args).toEqual({ provider: "claude" });
        return info;
      }
    });
    expect(await new TauriTransport().terminals.launch("claude")).toEqual(info);
  });

  it("lista, escribe, redimensiona y se conecta por id", async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === "list_terminals") return [];
      return null;
    });
    const t = new TauriTransport().terminals;
    expect(await t.list()).toEqual([]);
    await t.input("pty-1", "hola\r");
    await t.resize("pty-1", 120, 40);
    await t.attach("pty-1", () => {});
    expect(calls.map(([c]) => c)).toEqual([
      "list_terminals",
      "terminal_input",
      "terminal_resize",
      "attach_terminal",
    ]);
    expect(calls[1]?.[1]).toEqual({ id: "pty-1", data: "hola\r" });
    expect(calls[2]?.[1]).toEqual({ id: "pty-1", cols: 120, rows: 40 });
    expect(calls[3]?.[1]).toHaveProperty("onData");
  });
});
