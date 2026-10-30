//! `ProjectResolver` sobre el disco: busca el `.git` más cercano y lo recuerda por `cwd`.

use std::collections::HashMap;
use std::path::Path;

use crate::domain::{project_root, worktree_main_repo};
use crate::ports::ProjectResolver;

/// Con más carpetas distintas que esto se vacía la caché (no merece una LRU).
const CACHE_MAX: usize = 512;

#[derive(Debug, Default)]
pub struct GitProjects {
    cache: HashMap<String, String>,
    repos: HashMap<String, String>,
}

impl ProjectResolver for GitProjects {
    fn project_of(&mut self, cwd: &str) -> String {
        if let Some(root) = self.cache.get(cwd) {
            return root.clone();
        }
        if self.cache.len() >= CACHE_MAX {
            self.cache.clear();
        }
        let root = project_root(cwd, |dir: &Path| dir.join(".git").exists());
        self.cache.insert(cwd.to_owned(), root.clone());
        root
    }

    fn repo_of(&mut self, cwd: &str) -> String {
        if let Some(repo) = self.repos.get(cwd) {
            return repo.clone();
        }
        let root = self.project_of(cwd);
        let repo = std::fs::read_to_string(Path::new(&root).join(".git"))
            .ok()
            .and_then(|content| worktree_main_repo(&content))
            .unwrap_or(root);
        if self.repos.len() >= CACHE_MAX {
            self.repos.clear();
        }
        self.repos.insert(cwd.to_owned(), repo.clone());
        repo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encuentra_el_repositorio_en_disco() {
        let root = std::env::temp_dir().join(format!("kiti-git-roots-{}", std::process::id()));
        let sub = root.join("repo/packages/api");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(root.join("repo/.git")).unwrap();
        let mut projects = GitProjects::default();
        let expected = root.join("repo").to_string_lossy().into_owned();
        assert_eq!(projects.project_of(&sub.to_string_lossy()), expected);
        // De la caché, aunque el repositorio ya no exista.
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(projects.project_of(&sub.to_string_lossy()), expected);
    }

    #[test]
    fn un_worktree_comparte_repositorio_con_el_principal() {
        let root = std::env::temp_dir().join(format!("kiti-git-wt-{}", std::process::id()));
        let main = root.join("repo");
        let wt = root.join("repo-x");
        std::fs::create_dir_all(main.join(".git/worktrees/x")).unwrap();
        std::fs::create_dir_all(wt.join("src")).unwrap();
        std::fs::write(
            wt.join(".git"),
            format!("gitdir: {}\n", main.join(".git/worktrees/x").display()),
        )
        .unwrap();
        let mut projects = GitProjects::default();
        let main_s = main.to_string_lossy().into_owned();
        let wt_s = wt.to_string_lossy().into_owned();
        assert_eq!(projects.repo_of(&wt.join("src").to_string_lossy()), main_s);
        assert_eq!(projects.repo_of(&main_s), main_s);
        // Las zonas del mapa siguen separadas.
        assert_eq!(projects.project_of(&wt_s), wt_s);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
