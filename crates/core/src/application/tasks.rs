//! Agentes efímeros (ADR 0019): validación de `kiti_spawn`, mensajes entre el orquestador y el
//! agente de la tarea, y proyección al tablero.

use crate::domain::{Provider, Task, TaskRole, TaskState};
use kiti_protocol as proto;
use kiti_protocol::mcp;

/// Largo máximo del encargo (caracteres): va en un archivo, no en un mensaje.
pub const MAX_BRIEF: usize = 32_000;
/// Resumen de `kiti_done` que viaja en el mensaje al orquestador.
pub const MAX_SUMMARY: usize = 1_000;
const MAX_SLUG: usize = 40;
const MAX_MODEL: usize = 40;
const BRANCH_TYPES: [&str; 7] = ["feat", "fix", "docs", "test", "perf", "chore", "refactor"];

/// Petición de `kiti_spawn` ya validada: todo lo que acaba en un comando o en una ruta ha
/// pasado por una lista de caracteres permitidos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnPlan {
    pub role: TaskRole,
    pub provider: Provider,
    pub model: String,
    pub branch: String,
    pub brief: String,
}

pub fn error(code: &str, message: impl Into<String>) -> mcp::ErrorReply {
    mcp::ErrorReply {
        code: code.to_owned(),
        message: message.into(),
    }
}

fn bad(message: &str) -> mcp::ErrorReply {
    error("bad_request", message)
}

fn word(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.chars().count() <= max
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

fn domain_provider(p: Option<proto::Provider>) -> Option<Provider> {
    match p.unwrap_or(proto::Provider::Claude) {
        proto::Provider::Claude => Some(Provider::Claude),
        proto::Provider::Codex => Some(Provider::Codex),
        proto::Provider::Antigravity => Some(Provider::Antigravity),
        proto::Provider::Gemini => None,
    }
}

/// Modelo por defecto: el más barato que sirve. Solo Claude acepta elegirlo.
fn default_model(p: Provider) -> &'static str {
    match p {
        Provider::Claude => "sonnet",
        _ => "default",
    }
}

pub fn plan(r: &mcp::SpawnRequest) -> Result<SpawnPlan, mcp::ErrorReply> {
    let role = TaskRole::parse(&r.role)
        .ok_or_else(|| bad("role debe ser `implementar`, `documentar` o `revisar`"))?;
    let provider = domain_provider(r.provider)
        .ok_or_else(|| bad("proveedor no soportado: usa `claude`, `codex` o `agy`"))?;
    let branch_type = r.branch_type.trim();
    if !BRANCH_TYPES.contains(&branch_type) {
        return Err(bad(&format!(
            "branchType debe ser uno de: {}",
            BRANCH_TYPES.join(", ")
        )));
    }
    let slug = r.slug.trim();
    if !word(slug, MAX_SLUG) {
        return Err(bad(
            "slug: solo minúsculas, números y guiones (hasta 40 caracteres), p. ej. `cuota-en-vivo`",
        ));
    }
    let model = match r.model.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(m) => {
            let ok = m.chars().count() <= MAX_MODEL
                && m.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
            if !ok {
                return Err(bad("model: solo letras, números, `.`, `-` y `_`"));
            }
            m.to_owned()
        }
        None => default_model(provider).to_owned(),
    };
    let brief = r.task.trim();
    if brief.is_empty() {
        return Err(bad("task está vacío"));
    }
    if brief.chars().count() > MAX_BRIEF {
        return Err(bad(&format!("task supera los {MAX_BRIEF} caracteres")));
    }
    Ok(SpawnPlan {
        role,
        provider,
        model,
        branch: format!("{branch_type}/{slug}"),
        brief: brief.to_owned(),
    })
}

/// Lo primero que recibe el agente de la tarea, enviado en nombre del orquestador.
pub fn role_message(role: TaskRole, brief_path: &str) -> String {
    format!(
        "Tu rol: docs/agentes/{}.md (y comun.md). Tu encargo: {brief_path}. Léelos enteros y \
         síguelos. Al terminar, llama a kiti_done.",
        role.as_str()
    )
}

fn cut(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}…")
}

/// Aviso al orquestador cuando el agente de la tarea entrega.
pub fn done_message(
    task: &Task,
    outcome: &Result<Option<String>, String>,
    summary: &str,
) -> String {
    let head = match outcome {
        Ok(Some(pr)) => format!(
            "Tarea {} entregada (rama {}): PR {}.",
            task.id.0,
            task.branch,
            cut(pr, 200)
        ),
        Ok(None) => format!(
            "Tarea {} entregada (rama {}), sin PR.",
            task.id.0, task.branch
        ),
        Err(reason) => format!(
            "Tarea {} fallida (rama {}): {}.",
            task.id.0,
            task.branch,
            cut(reason, 300)
        ),
    };
    let summary = cut(summary, MAX_SUMMARY);
    if summary.is_empty() {
        head
    } else {
        format!("{head}\nResumen: {summary}")
    }
}

fn provider(p: Provider) -> proto::Provider {
    match p {
        Provider::Claude => proto::Provider::Claude,
        Provider::Codex => proto::Provider::Codex,
        Provider::Gemini => proto::Provider::Gemini,
        Provider::Antigravity => proto::Provider::Antigravity,
    }
}

fn role(r: TaskRole) -> proto::TaskRole {
    match r {
        TaskRole::Implementar => proto::TaskRole::Implementar,
        TaskRole::Documentar => proto::TaskRole::Documentar,
        TaskRole::Revisar => proto::TaskRole::Revisar,
    }
}

/// Una tarea para el tablero. `alias` es el del agente de la tarea, si ya está en el mapa.
pub fn task_info(t: &Task, alias: Option<String>) -> proto::TaskInfo {
    let (state, pr, reason) = match &t.state {
        TaskState::Launching => (proto::TaskStateKind::Launching, None, None),
        TaskState::Running => (proto::TaskStateKind::Running, None, None),
        TaskState::Done { pr } => (proto::TaskStateKind::Done, pr.clone(), None),
        TaskState::Failed { reason } => (proto::TaskStateKind::Failed, None, Some(reason.clone())),
        TaskState::Stuck => (proto::TaskStateKind::Stuck, None, None),
    };
    proto::TaskInfo {
        id: t.id.0,
        alias,
        orchestrator: t.orchestrator_alias.clone(),
        role: role(t.role),
        provider: provider(t.provider),
        model: t.model.clone(),
        branch: t.branch.clone(),
        session: t.session.clone(),
        pr,
        state,
        reason,
        created_at: t.created_at.0,
        finished_at: t.finished_at.map(|f| f.0),
    }
}

pub fn tasks_msg(tasks: Vec<proto::TaskInfo>) -> proto::ServerMsg {
    proto::ServerMsg::Tasks { tasks }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AgentId, Pid, ProcessStartTime, TaskId, Timestamp};

    fn request() -> mcp::SpawnRequest {
        mcp::SpawnRequest {
            role: "implementar".into(),
            task: "Haz X".into(),
            slug: "cuota-en-vivo".into(),
            branch_type: "feat".into(),
            provider: None,
            model: None,
        }
    }

    #[test]
    fn plan_valido_con_valores_por_defecto() {
        let p = plan(&request()).unwrap();
        assert_eq!(p.branch, "feat/cuota-en-vivo");
        assert_eq!(p.provider, Provider::Claude);
        assert_eq!(p.model, "sonnet");
        assert_eq!(p.role, TaskRole::Implementar);
    }

    #[test]
    fn rechaza_lo_que_acabaria_en_un_comando_peligroso() {
        for slug in ["", "-x", "x-", "a b", "a;rm", "../x", "A", "x/y", "$(id)"] {
            let mut r = request();
            r.slug = slug.into();
            assert_eq!(plan(&r).unwrap_err().code, "bad_request", "{slug}");
        }
        let mut r = request();
        r.model = Some("sonnet; rm -rf".into());
        assert_eq!(plan(&r).unwrap_err().code, "bad_request");
        let mut r = request();
        r.branch_type = "release".into();
        assert!(plan(&r).is_err());
        let mut r = request();
        r.role = "otro".into();
        assert!(plan(&r).is_err());
        let mut r = request();
        r.provider = Some(proto::Provider::Gemini);
        assert!(plan(&r).is_err());
        let mut r = request();
        r.task = "   ".into();
        assert!(plan(&r).is_err());
    }

    #[test]
    fn modelo_elegido_y_proveedor_sin_modelo() {
        let mut r = request();
        r.model = Some("claude-opus-5-5".into());
        assert_eq!(plan(&r).unwrap().model, "claude-opus-5-5");
        let mut r = request();
        r.provider = Some(proto::Provider::Antigravity);
        assert_eq!(plan(&r).unwrap().model, "default");
    }

    #[test]
    fn mensaje_de_rol() {
        assert_eq!(
            role_message(TaskRole::Revisar, "/tmp/t/4.md"),
            "Tu rol: docs/agentes/revisar.md (y comun.md). Tu encargo: /tmp/t/4.md. Léelos \
             enteros y síguelos. Al terminar, llama a kiti_done."
        );
    }

    fn task() -> Task {
        Task {
            id: TaskId(4),
            orchestrator: AgentId {
                provider: Provider::Claude,
                pid: Pid(1),
                start_time: ProcessStartTime(1),
            },
            orchestrator_alias: "claude-1".into(),
            role: TaskRole::Implementar,
            provider: Provider::Claude,
            model: "sonnet".into(),
            branch: "feat/x".into(),
            worktree: None,
            session: Some("kiti-3".into()),
            pid: Some(Pid(9)),
            brief_path: None,
            state: TaskState::Running,
            created_at: Timestamp(5),
            running_since: Some(Timestamp(5)),
            finished_at: None,
        }
    }

    #[test]
    fn aviso_de_entrega_con_pr_o_fallo() {
        let t = task();
        let ok = done_message(&t, &Ok(Some("#40".into())), "todo listo");
        assert!(ok.starts_with("Tarea 4 entregada (rama feat/x): PR #40."));
        assert!(ok.ends_with("Resumen: todo listo"));
        let fail = done_message(&t, &Err("no compila".into()), "");
        assert_eq!(fail, "Tarea 4 fallida (rama feat/x): no compila.");
        let long = done_message(&t, &Ok(None), &"x".repeat(5_000));
        assert!(long.chars().count() < 1_200);
    }

    #[test]
    fn proyeccion_al_tablero() {
        let mut t = task();
        let info = task_info(&t, Some("claude-2".into()));
        assert_eq!(info.state, proto::TaskStateKind::Running);
        assert_eq!(info.alias.as_deref(), Some("claude-2"));
        assert_eq!(info.orchestrator, "claude-1");
        t.state = TaskState::Done {
            pr: Some("#40".into()),
        };
        t.finished_at = Some(Timestamp(9));
        let info = task_info(&t, None);
        assert_eq!(info.pr.as_deref(), Some("#40"));
        assert_eq!(info.finished_at, Some(9));
        t.state = TaskState::Failed { reason: "x".into() };
        assert_eq!(task_info(&t, None).reason.as_deref(), Some("x"));
    }
}
