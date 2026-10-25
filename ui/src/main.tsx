import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { installDevHooks } from "./dev/kiti";
import { parseParams } from "./dev/params";
import { App } from "./panels/App";
import "./styles.css";
import { createKitiStore } from "./state/store";
import { createMetrics } from "./telemetry/metrics";
import { SimTransport } from "./transport/sim/SimTransport";
import type { WorldHandle } from "./world/World";

// Hito 1: solo SimTransport. TauriTransport llega en el Hito 2 (import dinámico).
const params = parseParams(window.location.search);
const store = createKitiStore();
const metrics = createMetrics();
const sim = new SimTransport({
  count: params.sim,
  seed: params.seed,
  steps: params.steps,
  freeze: params.freeze,
});

let world: WorldHandle | undefined;
const dev = installDevHooks(store, sim, metrics, () => world);

function connect() {
  void sim
    .connect((msg) => {
      store.getState().receive(msg);
      if (store.getState().agents.resync) {
        console.warn(JSON.stringify({ event: "transport.resync" }));
        sim.dispose();
        connect();
      }
    })
    .then((hello) => console.info(JSON.stringify({ event: "transport.connected", ...hello })));
}
connect();

const root = document.getElementById("root");
if (!root) throw new Error("#root missing");
createRoot(root).render(
  <StrictMode>
    <App
      store={store}
      transport={sim}
      metrics={metrics}
      params={params}
      now={() => sim.now()}
      onWorld={(w) => {
        world = w;
        dev.ready = w !== undefined;
      }}
    />
  </StrictMode>,
);
