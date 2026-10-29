//! Estado de git de un proyecto para el tablero de la oficina (ADR 0016).
//!
//! `git --no-optional-locks status --porcelain=v2 --branch`: sin `--no-optional-locks`, `status`
//! puede coger `index.lock` y hacer fallar el commit de un agente que trabaja en el mismo repo.

use std::path::Path;
use std::process::Command;

pub use crate::domain::GitStatus;

/// Salida de `git status --porcelain=v2 --branch`.
pub fn parse_porcelain_v2(out: &str) -> GitStatus {
    let mut s = GitStatus::default();
    for line in out.lines() {
        if let Some(head) = line.strip_prefix("# branch.head ") {
            s.branch = (head != "(detached)").then(|| head.to_owned());
        } else if let Some(ab) = line.strip_prefix("# branch.ab ") {
            let mut it = ab.split_whitespace();
            s.ahead = it
                .next()
                .and_then(|a| a.trim_start_matches('+').parse().ok())
                .unwrap_or(0);
            s.behind = it
                .next()
                .and_then(|b| b.trim_start_matches('-').parse().ok())
                .unwrap_or(0);
        } else if matches!(line.chars().next(), Some('1' | '2' | 'u' | '?')) {
            s.changed += 1;
        }
    }
    s
}

/// Variables con las que git ignora `-C`: las exporta un hook de git (Kiti lanzado desde uno, o
/// los tests en `pre-push`), y entonces `status` describiría otro repositorio.
const GIT_ENV: [&str; 4] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
];

/// `None` si no es un repositorio (o git falla).
pub fn status(dir: &Path) -> Option<GitStatus> {
    let mut git = Command::new("/usr/bin/git");
    for var in GIT_ENV {
        git.env_remove(var);
    }
    let out = git
        .args(["--no-optional-locks", "-C"])
        .arg(dir)
        .args(["status", "--porcelain=v2", "--branch"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| parse_porcelain_v2(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rama_adelanto_y_cambios() {
        let out = "# branch.oid 1234\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\n\
                   1 .M N... 100644 100644 100644 a b src/main.rs\n\
                   2 R. N... 100644 100644 100644 a b R100 nuevo.rs\tviejo.rs\n\
                   ? notas.txt\n";
        assert_eq!(
            parse_porcelain_v2(out),
            GitStatus {
                branch: Some("main".into()),
                ahead: 2,
                behind: 1,
                changed: 3,
            }
        );
    }

    #[test]
    fn head_separado_y_sin_upstream() {
        let out = "# branch.oid 1234\n# branch.head (detached)\n";
        assert_eq!(parse_porcelain_v2(out), GitStatus::default());
    }

    #[test]
    fn un_repositorio_real_y_una_carpeta_sin_git() {
        let root = std::env::temp_dir().join(format!("kiti-git-status-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(status(&root), None);
        let mut init = Command::new("/usr/bin/git");
        for var in GIT_ENV {
            init.env_remove(var);
        }
        let ok = init
            .args(["init", "-q", "-b", "trabajo"])
            .current_dir(&root)
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            std::fs::write(root.join("a.txt"), "x").unwrap();
            let s = status(&root).unwrap();
            assert_eq!(s.branch.as_deref(), Some("trabajo"));
            assert_eq!(s.changed, 1);
        }
        std::fs::remove_dir_all(&root).unwrap();
    }
}
