//! `ProjectResolver` sobre el disco: busca el `.git` más cercano y lo recuerda por `cwd`.

use std::collections::HashMap;
use std::path::Path;

use crate::domain::project_root;
use crate::ports::ProjectResolver;

/// Con más carpetas distintas que esto se vacía la caché (no merece una LRU).
const CACHE_MAX: usize = 512;

#[derive(Debug, Default)]
pub struct GitProjects {
    cache: HashMap<String, String>,
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
}
