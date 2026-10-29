import { useEffect, useRef } from "react";
import type { Params } from "../dev/params";
import type { KitiStore } from "../state/store";
import type { Metrics } from "../telemetry/metrics";
import { createWorld, type WorldHandle } from "../world/World";

type Props = {
  store: KitiStore;
  metrics: Metrics;
  params: Params;
  onWorld: (world: WorldHandle | undefined) => void;
  /** Reloj (el del simulador en modo sim) para la línea de tiempo del tablero. */
  now?: () => number;
};

/** Monta el mundo Pixi en un div. React nunca recibe datos por frame (ADR 0003 §3). */
export function WorldHost({ store, metrics, params, onWorld, now }: Props) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const opts = { metrics, anim: params.anim, zoom: params.zoom, ...(now ? { now } : {}) };
    const world = createWorld(el, store, params.focus ? { ...opts, focus: params.focus } : opts);
    let alive = true;
    void world.ready.then(() => {
      if (!alive) return;
      el.dataset.worldReady = "true";
      onWorld(world);
      console.info(JSON.stringify({ event: "world.ready", frames: world.frames() }));
    });
    return () => {
      alive = false;
      delete el.dataset.worldReady;
      onWorld(undefined);
      world.dispose();
    };
  }, [store, metrics, params, onWorld, now]);

  return <div ref={ref} data-testid="world" style={{ position: "absolute", inset: 0 }} />;
}
