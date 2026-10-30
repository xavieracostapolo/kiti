import { useEffect, useState } from "react";
import type { KitiStore } from "../state/store";
import type { Metrics } from "../telemetry/metrics";
import type { Transport } from "../transport/Transport";
import s from "./panels.module.css";

type Props = { metrics: Metrics; store: KitiStore; transport: Transport };

const mb = (bytes: number) => `${(bytes / 1_048_576).toFixed(1)} MB`;

/** Cmd+Shift+D. Solo muestrea a 1 Hz y pide el stream del core mientras está abierto. */
export function DebugPanel({ metrics, store, transport }: Props) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() === "d" && e.shiftKey && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    if (!open) return;
    void transport.setDebugStream(true);
    const show = () => {
      const core = store.getState().coreMetrics;
      const coreText = core
        ? {
            "core.rss": mb(core.rssBytes),
            "core.cpu_percent": core.cpuPercent.toFixed(2),
            "core.events_per_sec": core.eventsPerSec.toFixed(1),
            "core.agents": core.agents,
            "core.mails_total": core.mailsTotal,
            "core.mail_pending": core.mailPending,
            "core.memories_total": core.memoriesTotal,
            "core.memories_stored": core.memoriesStored,
            "core.memories_recalled": core.memoriesRecalled,
          }
        : { core: "sin datos (modo sim del navegador)" };
      setText(JSON.stringify({ ...coreText, ...metrics.snapshot() }, null, 1));
    };
    show();
    const h = setInterval(show, 1000);
    return () => {
      clearInterval(h);
      void transport.setDebugStream(false);
    };
  }, [open, metrics, store, transport]);

  return open ? <pre className={s.debug}>{text}</pre> : null;
}
