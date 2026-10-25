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
