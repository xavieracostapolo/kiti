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
            project: "/w/thoth-assets".into(),
            cwd: cwd.into(),
            title: title.map(Into::into),
        }
    }

    #[test]
    fn titulo_y_cuerpo_de_cada_aviso() {
        let n = notice(
            AlertKind::Finished,
            "/w/thoth-assets",
            Some("Migración de datos"),
        );
        assert_eq!(
            alert_text(&n),
            (
                "Codex terminó".into(),
                "thoth-assets · Migración de datos".into()
            )
        );
        let n = notice(AlertKind::Crashed, "/w/thoth-assets/api", None);
        assert_eq!(
            alert_text(&n),
            (
                "Codex se cerró de forma inesperada".into(),
                "thoth-assets/api".into()
            )
        );
        assert_eq!(
            alert_text(&notice(AlertKind::Failed, "/w/thoth-assets", None)).0,
            "Codex falló"
        );
    }
}
