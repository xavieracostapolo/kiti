import { useEffect, useState } from "react";
import type { Metrics } from "../telemetry/metrics";
import s from "./panels.module.css";

/** Cmd+Shift+D. Solo muestrea a 1 Hz mientras está abierto, para no pagar nada en reposo. */
export function DebugPanel({ metrics }: { metrics: Metrics }) {
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
    const show = () => setText(JSON.stringify(metrics.snapshot(), null, 1));
    show();
    const h = setInterval(show, 1000);
    return () => clearInterval(h);
  }, [open, metrics]);

  return open ? <pre className={s.debug}>{text}</pre> : null;
}
