//! Clasificación de procesos (ADR 0002 §2, ADR 0005 §4). Puro: recibe snapshots ya leídos.
//!
//! Se decide por `exe` y `argv`, nunca por el nombre del proceso (Gemini corre como `node`).

use std::collections::{HashMap, HashSet};

use super::agent::DetectedAgent;
use super::ids::{Pid, ProcessIdentity, ProcessStartTime, Provider, WorkingDir};

/// Lo que el escaneo lee de un proceso del usuario actual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSnapshot {
    pub pid: Pid,
    pub ppid: Option<Pid>,
    /// Epoch ms.
    pub start_time: ProcessStartTime,
    pub uid: u32,
    /// Ruta real del ejecutable (`proc_pidpath`). `None` si no se pudo leer.
    pub exe: Option<String>,
    pub argv: Vec<String>,
    pub cwd: Option<String>,
}

/// Codex con npm y Gemini lanzan dos procesos por sesión: solo uno es el agente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Agent,
    Launcher,
}

/// Subcomandos de Codex que no son una sesión de agente.
const CODEX_NOT_AGENT: [&str; 6] = [
    "app-server",
    "mcp-server",
    "login",
    "logout",
    "completion",
    "help",
];

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Primer argumento que no es una opción de node (`--max-old-space-size=…`).
fn node_script(argv: &[String]) -> Option<&str> {
    argv.iter()
        .skip(1)
        .map(String::as_str)
        .find(|a| !a.starts_with('-'))
}

fn is_gemini_script(script: &str) -> bool {
    script.ends_with("/bin/gemini")
        || script.ends_with("@google/gemini-cli/bundle/gemini.js")
        || (script.contains("/_npx/") && script.ends_with("/gemini"))
}

/// Filtro barato por la ruta del ejecutable, antes de copiar `argv` y `cwd`: si devuelve
/// `false`, `classify` nunca devolvería un agente ni un lanzador para ese proceso.
pub fn is_candidate_exe(exe: &str) -> bool {
    let name = basename(exe);
    name == "claude"
        || name == "codex"
        || name.starts_with("node")
        || exe.contains("/.local/share/claude/versions/")
        || exe.ends_with("/claude.exe")
}

pub fn classify(p: &ProcessSnapshot) -> Option<(Provider, Role)> {
    let exe = p.exe.as_deref()?;
    // Apps de escritorio (Claude.app, Gemini.app, Codex.app): no son CLIs de agente.
    if exe.contains(".app/Contents/") {
        return None;
    }
    let name = basename(exe);
    // sysinfo devuelve la ruta tal como se lanzó (`~/.local/bin/claude`, un symlink), no la
    // versión resuelta (`~/.local/share/claude/versions/<semver>`): se aceptan las dos.
    if name == "claude"
        || exe.contains("/.local/share/claude/versions/")
        || exe.ends_with("/@anthropic-ai/claude-code/bin/claude.exe")
    {
        return Some((Provider::Claude, Role::Agent));
    }
    if name == "codex" {
        let sub = p.argv.get(1).map(String::as_str);
        return match sub {
            Some(s) if CODEX_NOT_AGENT.contains(&s) => None,
            _ => Some((Provider::Codex, Role::Agent)),
        };
    }
    if name == "node" || name.starts_with("node") {
        let script = node_script(&p.argv)?;
        if script.ends_with("@openai/codex/bin/codex.js") {
            return Some((Provider::Codex, Role::Launcher));
        }
        if script.ends_with("@anthropic-ai/claude-code/cli.js") {
            return Some((Provider::Claude, Role::Agent));
        }
        if is_gemini_script(script) {
            return Some((Provider::Gemini, Role::Agent));
        }
    }
    None
}

/// Agentes reales: descarta lanzadores y, en cada cadena padre → hijo del mismo provider
/// (Gemini se relanza a sí mismo), se queda con el más profundo. Es el único sitio que clasifica.
pub fn dedupe_tree(snapshots: &[ProcessSnapshot]) -> Vec<(ProcessSnapshot, Provider)> {
    let parents: HashMap<Pid, Pid> = snapshots
        .iter()
        .filter_map(|s| s.ppid.map(|pp| (s.pid, pp)))
        .collect();
    let agents: HashMap<Pid, (&ProcessSnapshot, Provider)> = snapshots
        .iter()
        .filter_map(|s| match classify(s) {
            Some((provider, Role::Agent)) => Some((s.pid, (s, provider))),
            _ => None,
        })
        .collect();
    let mut superseded = HashSet::new();
    for (pid, (_, provider)) in &agents {
        let mut seen = HashSet::from([*pid]);
        let mut cur = *pid;
        while let Some(parent) = parents.get(&cur).copied() {
            if !seen.insert(parent) {
                break; // ciclo defensivo
            }
            if matches!(agents.get(&parent), Some((_, p)) if p == provider) {
                superseded.insert(parent);
            }
            cur = parent;
        }
    }
    let mut out: Vec<(ProcessSnapshot, Provider)> = agents
        .into_iter()
        .filter(|(pid, _)| !superseded.contains(pid))
        .map(|(_, (s, p))| (s.clone(), p))
        .collect();
    out.sort_by_key(|(s, _)| s.pid);
    out
}

/// FNV-1a de 64 bits sobre `argv`: estable y sin dependencias, para verificar antes del kill
/// que el PID sigue siendo el mismo programa.
fn fingerprint(argv: &[String]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in argv.iter().flat_map(|a| a.bytes().chain([0])) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

impl ProcessSnapshot {
    pub fn identity(&self) -> ProcessIdentity {
        ProcessIdentity {
            pid: self.pid,
            start_time: self.start_time,
            uid: self.uid,
            cmd_fingerprint: fingerprint(&self.argv),
        }
    }

    pub fn to_detected(&self, provider: Provider) -> DetectedAgent {
        DetectedAgent {
            provider,
            identity: self.identity(),
            cwd: WorkingDir::new(self.cwd.clone().unwrap_or_else(|| "?".to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/Users/dev";

    fn snap(pid: u32, ppid: u32, exe: &str, argv: &[&str]) -> ProcessSnapshot {
        ProcessSnapshot {
            pid: Pid(pid),
            ppid: Some(Pid(ppid)),
            start_time: ProcessStartTime(1_000 + u64::from(pid)),
            uid: 501,
            exe: Some(exe.to_owned()),
            argv: argv.iter().map(|s| s.to_string()).collect(),
            cwd: Some(format!("{HOME}/proj-{pid}")),
        }
    }

    fn class(s: &ProcessSnapshot) -> Option<(Provider, Role)> {
        classify(s)
    }

    #[test]
    fn claude_nativo_por_su_ruta_de_versiones() {
        // Capturado en local: argv es solo "claude".
        let s = snap(
            46618,
            22893,
            &format!("{HOME}/.local/share/claude/versions/2.1.310"),
            &["claude"],
        );
        assert_eq!(class(&s), Some((Provider::Claude, Role::Agent)));
    }

    #[test]
    fn claude_por_el_symlink_que_ve_sysinfo() {
        // Capturado en local con SysinfoScanner: `exe` es el symlink, no la versión resuelta.
        let s = snap(
            9759,
            1332,
            &format!("{HOME}/.local/bin/claude"),
            &["claude"],
        );
        assert_eq!(class(&s), Some((Provider::Claude, Role::Agent)));
        let brew = snap(1, 0, "/opt/homebrew/bin/claude", &["claude"]);
        assert_eq!(class(&brew), Some((Provider::Claude, Role::Agent)));
    }

    #[test]
    fn claude_por_npm_es_binario_nativo() {
        let exe = "/opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/bin/claude.exe";
        assert_eq!(
            class(&snap(1, 0, exe, &["claude"])),
            Some((Provider::Claude, Role::Agent))
        );
    }

    #[test]
    fn excluye_las_apps_de_escritorio() {
        for exe in [
            "/Applications/Claude.app/Contents/MacOS/Claude",
            "/Applications/Gemini.app/Contents/Helpers/GeminiAppLauncher.app/Contents/MacOS/GeminiAppLauncher",
            "/Applications/Codex.app/Contents/MacOS/codex",
            "/Applications/Claude.app/Contents/Resources/claude",
        ] {
            assert_eq!(class(&snap(1, 0, exe, &["x"])), None, "{exe}");
        }
    }

    #[test]
    fn codex_nativo_es_agente_y_el_lanzador_node_no() {
        let launcher = snap(
            10,
            1,
            "/opt/homebrew/bin/node",
            &[
                "node",
                "/opt/homebrew/lib/node_modules/@openai/codex/bin/codex.js",
            ],
        );
        let agent = snap(
            11,
            10,
            "/opt/homebrew/lib/node_modules/@openai/codex/vendor/aarch64-apple-darwin/codex/codex",
            &["codex"],
        );
        assert_eq!(class(&launcher), Some((Provider::Codex, Role::Launcher)));
        assert_eq!(class(&agent), Some((Provider::Codex, Role::Agent)));
    }

    #[test]
    fn codex_excluye_subcomandos_que_no_son_agentes() {
        for sub in ["app-server", "mcp-server", "login", "logout", "completion"] {
            let s = snap(1, 0, "/opt/homebrew/bin/codex", &["codex", sub]);
            assert_eq!(class(&s), None, "{sub}");
        }
        let exec = snap(
            1,
            0,
            "/opt/homebrew/bin/codex",
            &["codex", "exec", "arregla el test"],
        );
        assert_eq!(class(&exec), Some((Provider::Codex, Role::Agent)));
    }

    #[test]
    fn gemini_corre_como_node_y_se_reconoce_por_argv() {
        for script in [
            "/opt/homebrew/bin/gemini",
            "/opt/homebrew/lib/node_modules/@google/gemini-cli/bundle/gemini.js",
            "/Users/dev/.npm/_npx/9f8e7d/node_modules/.bin/gemini",
        ] {
            let s = snap(1, 0, "/opt/homebrew/bin/node", &["node", script]);
            assert_eq!(class(&s), Some((Provider::Gemini, Role::Agent)), "{script}");
        }
        // Gemini relanzado: node --max-old-space-size=… <script>
        let child = snap(
            2,
            1,
            "/opt/homebrew/bin/node",
            &[
                "node",
                "--max-old-space-size=8192",
                "/opt/homebrew/bin/gemini",
            ],
        );
        assert_eq!(class(&child), Some((Provider::Gemini, Role::Agent)));
    }

    #[test]
    fn un_node_cualquiera_no_es_un_agente() {
        for argv in [
            vec!["node", "/Users/dev/app/server.js"],
            vec!["node", "/Users/dev/gemini-clone/index.js"],
            vec!["node"],
        ] {
            assert_eq!(class(&snap(1, 0, "/opt/homebrew/bin/node", &argv)), None);
        }
    }

    #[test]
    fn el_prefiltro_por_exe_no_pierde_ningun_candidato() {
        for exe in [
            "/Users/dev/.local/bin/claude",
            "/Users/dev/.local/share/claude/versions/2.1.310",
            "/opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/bin/claude.exe",
            "/opt/homebrew/bin/codex",
            "/opt/homebrew/bin/node",
            "/Users/dev/.nvm/versions/node/v24.22.1/bin/node",
        ] {
            assert!(is_candidate_exe(exe), "{exe}");
        }
        for exe in [
            "/bin/zsh",
            "/usr/bin/ssh",
            "/Applications/Safari.app/Contents/MacOS/Safari",
        ] {
            assert!(!is_candidate_exe(exe), "{exe}");
        }
    }

    #[test]
    fn sin_exe_no_se_clasifica() {
        let mut s = snap(1, 0, "/x", &["claude"]);
        s.exe = None;
        assert_eq!(class(&s), None);
    }

    #[test]
    fn dedupe_se_queda_con_el_hijo_de_gemini_y_descarta_lanzadores() {
        let gemini_parent = snap(
            20,
            1,
            "/opt/homebrew/bin/node",
            &["node", "/opt/homebrew/bin/gemini"],
        );
        let gemini_child = snap(
            21,
            20,
            "/opt/homebrew/bin/node",
            &[
                "node",
                "--max-old-space-size=8192",
                "/opt/homebrew/bin/gemini",
            ],
        );
        let codex_launcher = snap(
            30,
            1,
            "/opt/homebrew/bin/node",
            &[
                "node",
                "/opt/homebrew/lib/node_modules/@openai/codex/bin/codex.js",
            ],
        );
        let codex_agent = snap(
            31,
            30,
            "/opt/homebrew/lib/node_modules/@openai/codex/vendor/x/codex/codex",
            &["codex"],
        );
        let claude = snap(
            40,
            1,
            "/Users/dev/.local/share/claude/versions/2.1.310",
            &["claude"],
        );
        let unrelated = snap(50, 1, "/bin/zsh", &["-zsh"]);

        let found = dedupe_tree(&[
            gemini_parent,
            gemini_child,
            codex_launcher,
            codex_agent,
            claude,
            unrelated,
        ]);
        let mut pids: Vec<(u32, Provider)> = found.iter().map(|(s, p)| (s.pid.0, *p)).collect();
        pids.sort();
        assert_eq!(
            pids,
            vec![
                (21, Provider::Gemini),
                (31, Provider::Codex),
                (40, Provider::Claude)
            ]
        );
    }

    #[test]
    fn dos_sesiones_independientes_del_mismo_cli_cuentan_por_separado() {
        let a = snap(
            60,
            1,
            "/Users/dev/.local/share/claude/versions/2.1.303",
            &["claude"],
        );
        let b = snap(
            61,
            1,
            "/Users/dev/.local/share/claude/versions/2.1.310",
            &["claude"],
        );
        assert_eq!(dedupe_tree(&[a, b]).len(), 2);
    }

    #[test]
    fn detected_agent_usa_la_identidad_del_proceso() {
        let s = snap(
            46618,
            1,
            "/Users/dev/.local/share/claude/versions/2.1.310",
            &["claude"],
        );
        let d = s.to_detected(Provider::Claude);
        assert_eq!(d.identity.pid, Pid(46618));
        assert_eq!(d.identity.start_time, s.start_time);
        assert_eq!(d.identity.uid, 501);
        assert_eq!(d.cwd.as_str(), "/Users/dev/proj-46618");
        // La huella de argv es estable y distingue procesos distintos.
        let mut other = s.clone();
        other.argv.push("--resume".into());
        assert_eq!(
            d.identity.cmd_fingerprint,
            s.to_detected(Provider::Claude).identity.cmd_fingerprint
        );
        assert_ne!(
            d.identity.cmd_fingerprint,
            other.to_detected(Provider::Claude).identity.cmd_fingerprint
        );
    }
}
