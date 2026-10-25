import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { installDevHooks } from "./dev/kiti";
import { parseParams } from "./dev/params";
import { App } from "./panels/App";
import "./styles.css";
import { createKitiStore } from "./state/store";
import { createMetrics } from "./telemetry/metrics";
import { startUiReport } from "./telemetry/report";
import { SimTransport } from "./transport/sim/SimTransport";
import type { Transport } from "./transport/Transport";
import type { WorldHandle } from "./world/World";

async function bootstrap() {
  const params = parseParams(window.location.search);
  const store = createKitiStore();
  const metrics = createMetrics();

  const isTauri =
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in window &&
    !window.location.search.includes("sim=");

  let transport: Transport;
  let getNow: () => number;
  let simAdvancer: { advance(n: number): void } | undefined;

  if (isTauri) {
    const { installLogBridge } = await import("./transport/tauri/logBridge");
    installLogBridge();
    const { TauriTransport } = await import("./transport/tauri/TauriTransport");
    transport = new TauriTransport();
    getNow = () => Date.now();
  } else {
    const sim = new SimTransport({
      count: params.sim,
      seed: params.seed,
      steps: params.steps,
      freeze: params.freeze,
    });
    transport = sim;
    getNow = () => sim.now();
    simAdvancer = sim;
  }

  let world: WorldHandle | undefined;
  const dev = installDevHooks(store, simAdvancer, metrics, () => world);

  function connect() {
    void transport
      .connect((msg) => {
        store.getState().receive(msg);
        if (store.getState().agents.resync) {
          console.warn(JSON.stringify({ event: "transport.resync" }));
          transport.dispose();
          connect();
        }
      })
      .then((hello) => {
        store.getState().setSource(hello.source);
        console.info(JSON.stringify({ event: "transport.connected", ...hello }));
      });
  }
  connect();
  // Resumen de la UI al core cada 2 s (no-op en Sim).
  startUiReport(metrics, (m) => transport.reportUiMetrics(m));

  const root = document.getElementById("root");
  if (!root) throw new Error("#root missing");
  createRoot(root).render(
    <StrictMode>
      <App
        store={store}
        transport={transport}
        metrics={metrics}
        params={params}
        now={getNow}
        onWorld={(w) => {
          world = w;
          dev.ready = w !== undefined;
        }}
      />
    </StrictMode>,
  );
}

void bootstrap();
