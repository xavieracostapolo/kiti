//! Proyecto de un agente (ADR 0009): la raíz del repositorio git que contiene su `cwd`, o el
//! propio `cwd` si no está en ninguno. Puro: quien llama dice dónde hay un `.git`.

use std::path::Path;

/// Sube desde `cwd` hasta encontrar un directorio con `.git` (carpeta, o archivo en los
/// worktrees). Sin repositorio, el proyecto es el propio `cwd`.
pub fn project_root(cwd: &str, has_git: impl Fn(&Path) -> bool) -> String {
    Path::new(cwd)
        .ancestors()
        .filter(|dir| dir.parent().is_some())
        .find(|dir| has_git(dir))
        .map_or_else(|| cwd.to_owned(), |dir| dir.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repos(roots: &'static [&'static str]) -> impl Fn(&Path) -> bool {
        move |dir| roots.iter().any(|r| Path::new(r) == dir)
    }

    #[test]
    fn un_subdirectorio_pertenece_a_su_repositorio() {
        let has = repos(&["/Users/dev/thoth-assets"]);
        assert_eq!(
            project_root("/Users/dev/thoth-assets/packages/api", &has),
            "/Users/dev/thoth-assets"
        );
        assert_eq!(
            project_root("/Users/dev/thoth-assets", &has),
            "/Users/dev/thoth-assets"
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
