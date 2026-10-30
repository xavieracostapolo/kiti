//! Proyecto de un agente (ADR 0009): la raíz del repositorio git que contiene su `cwd`, o el
//! propio `cwd` si no está en ninguno. Puro: quien llama dice dónde hay un `.git`.

use std::path::Path;

/// Estado de git de un proyecto (ADR 0016): lo lee un adapter; aquí solo el valor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStatus {
    /// Rama actual; `None` con HEAD separado.
    pub branch: Option<String>,
    /// Commits por subir y por bajar respecto a su upstream (0 sin upstream).
    pub ahead: u32,
    pub behind: u32,
    /// Archivos con cambios (staged, sin stage o sin seguimiento).
    pub changed: u32,
}

/// Sube desde `cwd` hasta encontrar un directorio con `.git` (carpeta, o archivo en los
/// worktrees). Sin repositorio, el proyecto es el propio `cwd`.
pub fn project_root(cwd: &str, has_git: impl Fn(&Path) -> bool) -> String {
    Path::new(cwd)
        .ancestors()
        .filter(|dir| dir.parent().is_some())
        .find(|dir| has_git(dir))
        .map_or_else(|| cwd.to_owned(), |dir| dir.to_string_lossy().into_owned())
}

/// Raíz del repositorio principal a partir del contenido del archivo `.git` de un worktree
/// (`gitdir: <repo>/.git/worktrees/<nombre>`). `None` si no tiene esa forma (submódulos, etc.).
pub fn worktree_main_repo(git_file: &str) -> Option<String> {
    let gitdir = git_file.lines().find_map(|l| l.strip_prefix("gitdir:"))?;
    let worktrees = Path::new(gitdir.trim()).parent()?;
    if worktrees.file_name()? != "worktrees" {
        return None;
    }
    let dot_git = worktrees.parent()?;
    if dot_git.file_name()? != ".git" {
        return None;
    }
    let repo = dot_git.parent()?;
    repo.parent()?;
    Some(repo.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_worktree_apunta_a_su_repositorio_principal() {
        assert_eq!(
            worktree_main_repo("gitdir: /Users/dev/tienda-web/.git/worktrees/rama-x\n"),
            Some("/Users/dev/tienda-web".to_owned())
        );
    }

    #[test]
    fn un_gitdir_ajeno_a_worktrees_no_cuenta() {
        assert_eq!(
            worktree_main_repo("gitdir: /Users/dev/tienda-web/.git"),
            None
        );
        assert_eq!(
            worktree_main_repo("gitdir: /Users/dev/tienda-web/.git/modules/lib"),
            None
        );
        assert_eq!(worktree_main_repo("basura"), None);
        assert_eq!(worktree_main_repo(""), None);
    }

    fn repos(roots: &'static [&'static str]) -> impl Fn(&Path) -> bool {
        move |dir| roots.iter().any(|r| Path::new(r) == dir)
    }

    #[test]
    fn un_subdirectorio_pertenece_a_su_repositorio() {
        let has = repos(&["/Users/dev/tienda-web"]);
        assert_eq!(
            project_root("/Users/dev/tienda-web/packages/api", &has),
            "/Users/dev/tienda-web"
        );
        assert_eq!(
            project_root("/Users/dev/tienda-web", &has),
            "/Users/dev/tienda-web"
        );
    }

    #[test]
    fn el_repositorio_mas_cercano_gana() {
        let has = repos(&["/w", "/w/vendor/lib"]);
        assert_eq!(project_root("/w/vendor/lib/src", &has), "/w/vendor/lib");
    }

    #[test]
    fn sin_repositorio_el_proyecto_es_la_carpeta() {
        assert_eq!(project_root("/tmp/suelto", repos(&[])), "/tmp/suelto");
        // La raíz del disco nunca cuenta como proyecto.
        assert_eq!(project_root("/tmp/suelto", repos(&["/"])), "/tmp/suelto");
    }
}
