import { useStore } from "zustand";
import type { MailCommand, MailMessage, MailThread } from "../protocol";
import { chipOf } from "../state/mail";
import type { KitiStore } from "../state/store";
import { ago } from "./labels";
import s from "./panels.module.css";

type Props = {
  store: KitiStore;
  now: () => number;
  /** Envía la acción al core (o al simulador). */
  act: (command: MailCommand) => void;
};

/** Chip de la barra superior: abre el panel; amarillo solo si algo pide atención (ADR 0017). */
export function MailChip({ store }: { store: KitiStore }) {
  const mail = useStore(store, (st) => st.mail);
  const open = useStore(store, (st) => st.mailOpen);
  const chip = chipOf(mail);
  if (!chip) return null;
  const label = chip.attention
    ? `Mensajes: ${chip.count} por decidir`
    : `Mensajes entre agentes: ${chip.count} en camino`;
  return (
    <button
      type="button"
      className={`${s.mailChip} ${chip.attention ? s.mailChipAlert : ""}`}
      aria-label={label}
      aria-pressed={open}
      title={label}
      onClick={() => store.getState().setMailOpen(!open)}
    >
      ✉ {chip.count}
    </button>
  );
}

const STATE_LABEL: Record<MailMessage["state"], string> = {
  pending: "por aprobar",
  queued: "en cola",
  held: "retenido",
  delivered: "entregado",
  discarded: "descartado",
};

/**
 * Registro de los mensajes entre agentes (ADR 0017). En automático (por defecto) solo se mira;
 * pide atención un hilo pausado por el límite de saltos o, en manual, un mensaje por aprobar.
 */
export function MailPanel({ store, now, act }: Props) {
  const mail = useStore(store, (st) => st.mail);
  const open = useStore(store, (st) => st.mailOpen);
  if (!open || !mail) return null;
  const pending = mail.threads
    .flatMap((t) => t.messages)
    .filter((m) => m.state === "pending" || m.state === "queued" || m.state === "held").length;

  return (
    <aside className={s.mailPanel} aria-label="Mensajes entre agentes">
      <div className={s.mailHead}>
        <h2 className={s.mailTitle}>Mensajes</h2>
        <fieldset className={s.layoutGroup} aria-label="Modo de entrega">
          {(["auto", "manual"] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              aria-pressed={mail.mode === mode}
              className={`${s.layoutBtn} ${mail.mode === mode ? s.layoutActive : ""}`}
              onClick={() => act({ kind: "mode", mode })}
            >
              {mode === "auto" ? "Automático" : "Manual"}
            </button>
          ))}
        </fieldset>
        <button
          type="button"
          className={s.button}
          aria-pressed={mail.paused}
          title={mail.paused ? "Reanudar las entregas" : "Congelar todas las entregas"}
          onClick={() => act({ kind: "pause", paused: !mail.paused })}
        >
          {mail.paused ? "▶ Reanudar" : "⏸ Pausar"}
        </button>
        <button
          type="button"
          className={s.closeBtn}
          aria-label="Cerrar mensajes"
          onClick={() => store.getState().setMailOpen(false)}
        >
          ✕
        </button>
      </div>
      <div className={s.mailBody}>
        {mail.threads.length === 0 && (
          <p className={s.notice}>
            Aún no hay mensajes. Los agentes con <code>kiti-mcp</code> pueden escribirse con la
            herramienta <code>kiti_send</code> (<code>mise run mcp:install</code>).
          </p>
        )}
        {mail.threads.map((t) => (
          <Thread key={t.id} thread={t} now={now} act={act} />
        ))}
      </div>
      <div className={s.mailFoot}>
        <span>
          {mail.paused ? "En pausa" : mail.mode === "auto" ? "Automático" : "Manual"} · límites:
          20/h por agente, 12 saltos por hilo
        </span>
        <span>sin entregar: {pending}</span>
      </div>
    </aside>
  );
}

function participants(t: MailThread): string {
  const names: string[] = [];
  for (const m of t.messages) {
    for (const a of [m.fromAlias, m.toAlias]) if (!names.includes(a)) names.push(a);
  }
  return names.join(" ⇄ ");
}

function Thread({
  thread: t,
  now,
  act,
}: {
  thread: MailThread;
  now: () => number;
  act: (c: MailCommand) => void;
}) {
  const paused = t.state === "paused";
  return (
    <section
      className={`${s.mailThread} ${paused ? s.mailThreadAlert : ""}`}
      aria-label={`Hilo ${t.subject}`}
    >
      <header className={s.mailThreadHead}>
        <span>
          <strong>{t.subject}</strong> · {participants(t)}
        </span>
        <span>
          {paused ? "pausado" : t.state === "closed" ? "cerrado" : "salto"}{" "}
          {Math.min(t.hops, t.limit)}/{t.limit}
        </span>
      </header>
      {paused && (
        <div className={s.mailActions}>
          <span className={s.mailAsk}>Llegó al límite de saltos. ¿Lo dejas seguir?</span>
          <button
            type="button"
            className={`${s.button} ${s.ok}`}
            onClick={() => act({ kind: "continue", id: t.id })}
          >
            Continuar (+12)
          </button>
          <button
            type="button"
            className={`${s.button} ${s.danger}`}
            onClick={() => act({ kind: "close", id: t.id })}
          >
            Cerrar hilo
          </button>
        </div>
      )}
      {t.messages.map((m) => (
        <div
          key={m.id}
          className={`${s.mailMsg} ${m.state === "pending" ? s.mailPending : ""}`}
          data-state={m.state}
        >
          <div className={s.mailFrom}>
            <strong>{m.fromAlias}</strong> → {m.toAlias} · {ago(now() - m.sentAt)} ·{" "}
            {STATE_LABEL[m.state]}
          </div>
          <div className={s.mailText}>{m.text}</div>
          {m.state === "pending" && (
            <div className={s.mailActions}>
              <button
                type="button"
                className={`${s.button} ${s.ok}`}
                onClick={() => act({ kind: "approve", id: m.id })}
              >
                Entregar
              </button>
              <button
                type="button"
                className={`${s.button} ${s.danger}`}
                onClick={() => act({ kind: "discard", id: m.id })}
              >
                Descartar
              </button>
            </div>
          )}
        </div>
      ))}
    </section>
  );
}
