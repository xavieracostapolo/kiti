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
  if (params.layoutMode) store.getState().setLayoutMode(params.layoutMode);
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
      terminals: params.terminals,
      conversations: params.conversations,
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
        // Terminales de agentes lanzados antes de abrir esta ventana (ADR 0007).
        void transport.terminals?.list().then((list) => {
          store.getState().setTerminals(list);
          const agents = [...store.getState().agents.agents.values()];
          const matched = list.filter((t) => agents.some((a) => a.pid === t.pid)).length;
          console.info(
            JSON.stringify({
              event: "terminals.listed",
              count: list.length,
              matched,
              agents: agents.length,
            }),
          );
        });
        refreshConversations();
        console.info(JSON.stringify({ event: "transport.connected", ...hello }));
      });
  }
  // Conversaciones cerradas (ADR 0009): al conectar, cada 15 s y cuando se cierra una oficina.
  const conversations = transport.conversations;
  function refreshConversations() {
    void conversations
      ?.list()
      .then((list) => store.getState().setConversations(list))
      .catch((e) => console.warn(JSON.stringify({ event: "conversations.error", error: e })));
  }
  if (conversations) {
    setInterval(refreshConversations, 15_000);
    store.subscribe((s, prev) => {
      if (s.agents.agents.size < prev.agents.agents.size) refreshConversations();
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
