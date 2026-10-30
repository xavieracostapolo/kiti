//! Worktrees de git para las tareas de los agentes efímeros (ADR 0019). `git` se ejecuta con
//! argumentos separados, nunca por la shell, y la rama se valida antes de pasarla.

use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::info;

use super::git_status::GIT_ENV;

const MAX_BRANCH_LEN: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removal {
    Removed,
    Kept(String),
}

/// Solo `[a-z0-9/_-]`, sin `..`, sin empezar por `-` ni `/`, de 1 a 80 caracteres.
pub fn valid_branch(branch: &str) -> bool {
    !branch.is_empty()
        && branch.len() <= MAX_BRANCH_LEN
        && !branch.starts_with(['-', '/'])
        && !branch.contains("..")
        && branch
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'/' | b'_' | b'-'))
}

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidInput, msg.into())
}

/// Carpeta hermana de `repo`: `<nombre de repo>-<último segmento de branch>`.
fn sibling(repo: &Path, branch: &str) -> Result<PathBuf> {
    let name = repo
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| invalid("repo sin nombre"))?;
    let last = branch.rsplit('/').next().unwrap_or(branch);
    if last.is_empty() {
        return Err(invalid("rama sin último segmento"));
    }
    let parent = repo
        .parent()
        .ok_or_else(|| invalid("repo sin carpeta padre"))?;
    Ok(parent.join(format!("{name}-{last}")))
}

/// `git` en `dir`, sin las variables que un hook de git exporta (`GIT_DIR`...) y con las que git
/// ignora `-C`: actuaría sobre otro repositorio.
fn git_command(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    for var in GIT_ENV {
        cmd.env_remove(var);
    }
    cmd.arg("-C").arg(dir);
    cmd
}

fn git(dir: &Path, args: &[&str]) -> Result<std::process::Output> {
    git_command(dir).args(args).output()
}

fn git_ok(dir: &Path, args: &[&str]) -> Result<String> {
    let out = git(dir, args)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        let why = String::from_utf8_lossy(&out.stderr);
        Err(Error::other(format!(
            "git {}: {}",
            args.first().copied().unwrap_or(""),
            why.trim()
        )))
    }
}

fn git_succeeds(dir: &Path, args: &[&str]) -> bool {
    git(dir, args).is_ok_and(|o| o.status.success())
}

/// `git fetch origin main` y `git worktree add <hermano> -b <branch> origin/main`.
pub fn create(repo: &Path, branch: &str) -> Result<Worktree> {
    if !valid_branch(branch) {
        return Err(invalid("nombre de rama no válido"));
    }
    let path = sibling(repo, branch)?;
    if path.exists() {
        return Err(Error::new(
            ErrorKind::AlreadyExists,
            "la carpeta del worktree ya existe",
        ));
    }
    let heads = format!("refs/heads/{branch}");
    if git_succeeds(repo, &["show-ref", "--verify", "--quiet", &heads]) {
        return Err(Error::new(ErrorKind::AlreadyExists, "la rama ya existe"));
    }
    git_ok(repo, &["fetch", "origin", "main"])?;
    let path_arg = path.to_string_lossy().into_owned();
    git_ok(
        repo,
        &["worktree", "add", &path_arg, "-b", branch, "origin/main"],
    )?;
    info!(event = "worktree.created", reason = "task");
    Ok(Worktree {
        path,
        branch: branch.to_owned(),
    })
}

fn keep(reason: &str) -> Removal {
    info!(event = "worktree.kept", reason);
    Removal::Kept(reason.to_owned())
}

/// Borra el worktree y su rama local solo si no hay cambios sin commitear y la rama no tiene
/// commits propios sin subir. Nunca `--force`, nunca ramas remotas.
pub fn remove_if_safe(repo: &Path, worktree: &Path) -> Removal {
    let Ok(status) = git_ok(worktree, &["status", "--porcelain"]) else {
        return keep("no se pudo leer el estado del worktree");
    };
    if !status.is_empty() {
        return keep("tiene cambios sin commitear");
    }
    let Ok(branch) = git_ok(worktree, &["rev-parse", "--abbrev-ref", "HEAD"]) else {
        return keep("no se pudo leer la rama");
    };
    if branch == "HEAD" || !valid_branch(&branch) {
        return keep("la rama no es una rama de tarea");
    }
    let in_main = git_succeeds(
        repo,
        &["merge-base", "--is-ancestor", &branch, "origin/main"],
    );
    let pushed = git_ok(
        repo,
        &[
            "rev-list",
            "--count",
            &format!("{branch}@{{upstream}}..{branch}"),
        ],
    )
    .is_ok_and(|n| n == "0");
    if !in_main && !pushed {
        return keep("tiene commits que no están en su upstream ni en origin/main");
    }
    let path_arg = worktree.to_string_lossy().into_owned();
    if let Err(e) = git_ok(repo, &["worktree", "remove", &path_arg]) {
        return keep(&format!("git no pudo quitar el worktree: {e}"));
    }
    // La seguridad ya está comprobada arriba; `-d` rechazaría una rama subida pero sin fusionar.
    if let Err(e) = git_ok(repo, &["branch", "-D", &branch]) {
        return keep(&format!("worktree quitado, pero no la rama: {e}"));
    }
    info!(event = "worktree.removed", reason = "safe");
    Removal::Removed
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static N: AtomicU32 = AtomicU32::new(0);

    fn run(dir: &Path, args: &[&str]) {
        let out = git_command(dir)
            .args(["-c", "user.name=Dev", "-c", "user.email=dev@example.com"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Raíz temporal con `origin.git` (bare, con un commit en main) y el clon `shop`.
    struct Fixture {
        root: PathBuf,
        repo: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "kiti-worktree-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            let origin = root.join("origin.git");
            let seed = root.join("seed");
            let repo = root.join("shop");
            run(&root, &["init", "--bare", "-b", "main", "origin.git"]);
            run(&root, &["init", "-b", "main", "seed"]);
            std::fs::write(seed.join("a.txt"), "a").unwrap();
            run(&seed, &["add", "."]);
            run(&seed, &["commit", "-m", "init"]);
            run(&seed, &["push", origin.to_str().unwrap(), "main"]);
            run(&root, &["clone", origin.to_str().unwrap(), "shop"]);
            Self { root, repo }
        }

        fn commit_in(&self, dir: &Path, file: &str) {
            std::fs::write(dir.join(file), file).unwrap();
            run(dir, &["add", "."]);
            run(dir, &["commit", "-m", file]);
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn branch_validation() {
        for ok in ["feat/task-board", "a", "fix_1/x-2"] {
            assert!(valid_branch(ok), "{ok}");
        }
        let long = "a".repeat(81);
        for bad in [
            "", "-x", "/x", "a..b", "Feat/x", "a b", "a;rm", "a$(x)", "a\nb", "ñ", &long,
        ] {
            assert!(!valid_branch(bad), "{bad:?}");
        }
        assert!(valid_branch(&"a".repeat(80)));
    }

    #[test]
    fn create_makes_sibling_folder_and_branch() {
        let f = Fixture::new();
        let wt = create(&f.repo, "feat/task-board").unwrap();
        assert_eq!(wt.path, f.root.join("shop-task-board"));
        assert_eq!(wt.branch, "feat/task-board");
        assert!(wt.path.join("a.txt").exists());
        let head = git_ok(&wt.path, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap();
        assert_eq!(head, "feat/task-board");
    }

    #[test]
    fn create_fails_if_folder_exists() {
        let f = Fixture::new();
        std::fs::create_dir(f.root.join("shop-task-board")).unwrap();
        let err = create(&f.repo, "feat/task-board").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::AlreadyExists);
    }

    #[test]
    fn create_fails_if_branch_exists() {
        let f = Fixture::new();
        run(&f.repo, &["branch", "feat/task-board"]);
        let err = create(&f.repo, "feat/task-board").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::AlreadyExists);
        assert!(!f.root.join("shop-task-board").exists());
    }

    #[test]
    fn create_rejects_invalid_branch() {
        let f = Fixture::new();
        for bad in ["--upload-pack=x", "../x", "A", "a b", ""] {
            let err = create(&f.repo, bad).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{bad:?}");
        }
    }

    #[test]
    fn removes_clean_worktree_without_own_commits() {
        let f = Fixture::new();
        let wt = create(&f.repo, "feat/clean").unwrap();
        assert_eq!(remove_if_safe(&f.repo, &wt.path), Removal::Removed);
        assert!(!wt.path.exists());
        assert!(!git_succeeds(
            &f.repo,
            &["show-ref", "--verify", "--quiet", "refs/heads/feat/clean"]
        ));
    }

    #[test]
    fn keeps_worktree_with_uncommitted_changes() {
        let f = Fixture::new();
        let wt = create(&f.repo, "feat/dirty").unwrap();
        std::fs::write(wt.path.join("new.txt"), "x").unwrap();
        assert!(matches!(
            remove_if_safe(&f.repo, &wt.path),
            Removal::Kept(_)
        ));
        assert!(wt.path.exists());
    }

    #[test]
    fn keeps_worktree_with_unpushed_commit() {
        let f = Fixture::new();
        let wt = create(&f.repo, "feat/ahead").unwrap();
        f.commit_in(&wt.path, "b.txt");
        assert!(matches!(
            remove_if_safe(&f.repo, &wt.path),
            Removal::Kept(_)
        ));
        assert!(wt.path.exists());
    }

    #[test]
    fn removes_worktree_whose_commit_is_in_origin_main() {
        let f = Fixture::new();
        let wt = create(&f.repo, "feat/merged").unwrap();
        f.commit_in(&wt.path, "b.txt");
        // Simula el merge: main de origin avanza hasta ese commit.
        run(&wt.path, &["push", "origin", "HEAD:main"]);
        run(&f.repo, &["fetch", "origin", "main"]);
        assert_eq!(remove_if_safe(&f.repo, &wt.path), Removal::Removed);
        assert!(!wt.path.exists());
    }

    #[test]
    fn git_command_removes_hook_variables() {
        let cmd = git_command(Path::new("."));
        let envs: Vec<_> = cmd.get_envs().collect();
        for var in GIT_ENV {
            assert!(
                envs.iter().any(|(k, v)| *k == var && v.is_none()),
                "{var} no se quita"
            );
        }
    }
}
