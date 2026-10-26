//! Texto de los avisos (ADR 0010): título corto con el agente y qué pasa; cuerpo con el proyecto
//! y el título de la conversación. Nunca el pedido del usuario.

use crate::domain::{AlertKind, Provider};
use crate::ports::AlertNotice;

fn provider_name(p: Provider) -> &'static str {
    match p {
        Provider::Claude => "Claude",
        Provider::Codex => "Codex",
        Provider::Gemini => "Gemini",
        Provider::Antigravity => "Antigravity",
    }
}

fn last_dir(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
}

pub fn alert_text(n: &AlertNotice) -> (String, String) {
    let who = provider_name(n.provider);
    let what = match n.kind {
        AlertKind::Permission => "pide permiso",
        AlertKind::Finished => "terminó",
        AlertKind::Failed => "falló",
        AlertKind::Crashed => "se cerró de forma inesperada",
    };
    let mut body = last_dir(&n.project).to_owned();
    // En una subcarpeta del repositorio, se dice cuál.
    if n.cwd != n.project {
        body = format!("{body}/{}", last_dir(&n.cwd));
    }
    if let Some(title) = &n.title {
        body = format!("{body} · {title}");
    }
    (format!("{who} {what}"), body)
}

/// "en 3 d 4 h", "en 5 h", "en 20 min" hasta `at` (ms), o vacío si no se sabe.
pub fn time_until(at: u64, now: u64) -> String {
    if at == 0 {
        return String::new();
    }
    let min = at.saturating_sub(now) / 60_000;
    let (d, h, m) = (min / 1440, (min % 1440) / 60, min % 60);
    match (d, h) {
        (0, 0) => format!("en {m} min"),
        (0, _) => format!("en {h} h"),
        (_, 0) => format!("en {d} d"),
        _ => format!("en {d} d {h} h"),
    }
}

/// Aviso de cuota baja (ADR 0015): "Queda poca cuota de Claude" / "semana: 18 % · se reinicia en 2 d".
pub fn quota_text(alert: &crate::domain::QuotaAlert, now: u64) -> (String, String) {
    let w = &alert.window;
    let pct = (w.remaining * 100.0).round() as u32;
    let mut body = format!("{}: {pct} %", w.label);
    let reset = time_until(w.resets_at, now);
    if !reset.is_empty() {
        body = format!("{body} · se reinicia {reset}");
    }
    (
        format!("Queda poca cuota de {}", provider_name(alert.provider)),
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AgentId, Pid, ProcessStartTime};

    fn notice(kind: AlertKind, cwd: &str, title: Option<&str>) -> AlertNotice {
        AlertNotice {
            kind,
            agent: AgentId {
                provider: Provider::Codex,
                pid: Pid(1),
                start_time: ProcessStartTime(1),
            },
            provider: Provider::Codex,
            project: "/w/tienda-web".into(),
            cwd: cwd.into(),
            title: title.map(Into::into),
        }
    }

    #[test]
    fn titulo_y_cuerpo_de_cada_aviso() {
        let n = notice(
            AlertKind::Finished,
            "/w/tienda-web",
            Some("Migración de datos"),
        );
        assert_eq!(
            alert_text(&n),
            (
                "Codex terminó".into(),
                "tienda-web · Migración de datos".into()
            )
        );
        let n = notice(AlertKind::Crashed, "/w/tienda-web/api", None);
        assert_eq!(
            alert_text(&n),
            (
                "Codex se cerró de forma inesperada".into(),
                "tienda-web/api".into()
            )
        );
        assert_eq!(
            alert_text(&notice(AlertKind::Failed, "/w/tienda-web", None)).0,
            "Codex falló"
        );
    }

    #[test]
    fn aviso_de_cuota_con_lo_que_queda_y_cuando_se_reinicia() {
        let alert = crate::domain::QuotaAlert {
            provider: Provider::Antigravity,
            window: crate::domain::QuotaWindow {
                label: "Claude and GPT models · semana".into(),
                weekly: true,
                remaining: 0.18,
                resets_at: 3 * 86_400_000 + 4 * 3_600_000,
            },
        };
        assert_eq!(
            quota_text(&alert, 0),
            (
                "Queda poca cuota de Antigravity".into(),
                "Claude and GPT models · semana: 18 % · se reinicia en 3 d 4 h".into()
            )
        );
        assert_eq!(time_until(0, 5), "");
        assert_eq!(time_until(20 * 60_000, 0), "en 20 min");
        assert_eq!(time_until(5 * 3_600_000, 0), "en 5 h");
        assert_eq!(time_until(2 * 86_400_000, 0), "en 2 d");
    }
}
