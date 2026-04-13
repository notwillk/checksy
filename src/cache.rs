use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Parsed Git-based resource locator
#[derive(Debug, Clone, PartialEq)]
pub struct GitRemote {
    pub repo: String,
    pub ref_: String,
    pub path: String,
}

const DEFAULT_CACHE_PATH: &str = ".checksy-cache";
const GIT_CACHE_DIR: &str = "git";

/// Manages the .checksy-cache directory structure
pub struct CacheManager {
    /// Root of the cache directory (<config-dir>/<cache-path>)
    root: PathBuf,
}

impl CacheManager {
    /// Create a new CacheManager
    /// config_dir: directory containing the config file
    /// cache_path: optional override from config (defaults to ".checksy-cache")
    pub fn new(config_dir: &Path, cache_path: Option<&str>) -> Self {
        let cache_path = cache_path.unwrap_or(DEFAULT_CACHE_PATH);
        let root = config_dir.join(cache_path);
        Self { root }
    }

    /// Get the root cache path
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// URL-safe encoding for repository names
    /// Converts URL characters that are invalid in directory names
    /// e.g., "https://github.com/org/repo.git" -> "https___github.com_org_repo.git"
    pub fn encode_repo_name(url: &str) -> String {
        url.chars()
            .map(|c| match c {
                '/' => '_',
                ':' => '_',
                '?' => '_',
                '&' => '_',
                '=' => '_',
                '#' => '_',
                ' ' => '_',
                c => c,
            })
            .collect()
    }

    /// Get the path to the git cache directory for a specific repo
    fn repo_cache_path(&self, repo: &str) -> PathBuf {
        let encoded = Self::encode_repo_name(repo);
        self.root.join(GIT_CACHE_DIR).join(encoded)
    }

    /// Get the path to a specific ref's clone directory
    pub fn ref_cache_path(&self, repo: &str, ref_: &str) -> PathBuf {
        // Sanitize ref name for directory (replace path separators)
        let safe_ref = ref_.replace(['/', '\\'], "_");
        self.repo_cache_path(repo).join(safe_ref)
    }

    /// Check if a specific git remote is already cached
    pub fn is_cached(&self, repo: &str, ref_: &str) -> bool {
        let path = self.ref_cache_path(repo, ref_);
        // Check if the directory exists and contains a .git directory
        path.join(".git").is_dir()
    }

    /// Get the absolute path to the config file in a cached git remote
    pub fn get_config_path(&self, git_remote: &GitRemote) -> PathBuf {
        let base_path = self.ref_cache_path(&git_remote.repo, &git_remote.ref_);
        base_path.join(&git_remote.path)
    }

    /// Collect all currently cached refs for a repo
    fn list_cached_refs(&self, repo: &str) -> Vec<String> {
        let repo_path = self.repo_cache_path(repo);
        if !repo_path.exists() {
            return vec![];
        }

        let mut refs = vec![];
        if let Ok(entries) = fs::read_dir(&repo_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join(".git").exists() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        refs.push(name.to_string());
                    }
                }
            }
        }
        refs
    }

    /// Collect all cached repos
    fn list_cached_repos(&self) -> Vec<String> {
        let git_path = self.root.join(GIT_CACHE_DIR);
        if !git_path.exists() {
            return vec![];
        }

        let mut repos = vec![];
        if let Ok(entries) = fs::read_dir(&git_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        repos.push(name.to_string());
                    }
                }
            }
        }
        repos
    }

    /// Remove unused cached refs and repos
    /// used: set of (encoded_repo, ref) tuples that are currently in use
    pub fn prune(&self, used: &HashSet<(String, String)>) -> Result<(), String> {
        if !self.root.exists() {
            return Ok(());
        }

        let git_path = self.root.join(GIT_CACHE_DIR);
        if !git_path.exists() {
            return Ok(());
        }

        // Collect all cached repos
        let cached_repos = self.list_cached_repos();

        for encoded_repo in cached_repos {
            let repo_path = git_path.join(&encoded_repo);
            let cached_refs = self.list_cached_refs_from_path(&repo_path);

            for ref_name in cached_refs {
                let key = (encoded_repo.clone(), ref_name.clone());
                if !used.contains(&key) {
                    // This ref is not used, remove it
                    let ref_path = repo_path.join(&ref_name);
                    if let Err(e) = fs::remove_dir_all(&ref_path) {
                        return Err(format!(
                            "failed to remove unused cache {}: {}",
                            ref_path.display(),
                            e
                        ));
                    }
                }
            }

            // If repo directory is now empty, remove it too
            if let Ok(mut entries) = fs::read_dir(&repo_path) {
                if entries.next().is_none() {
                    let _ = fs::remove_dir(&repo_path);
                }
            }
        }

        Ok(())
    }

    /// Helper to list refs from a specific repo path
    fn list_cached_refs_from_path(&self, repo_path: &Path) -> Vec<String> {
        let mut refs = vec![];
        if let Ok(entries) = fs::read_dir(repo_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join(".git").exists() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        refs.push(name.to_string());
                    }
                }
            }
        }
        refs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_encode_repo_name() {
        assert_eq!(
            CacheManager::encode_repo_name("https://github.com/org/repo.git"),
            "https___github.com_org_repo.git"
        );
        assert_eq!(
            CacheManager::encode_repo_name("https://gitlab.com/user/project?foo=bar"),
            "https___gitlab.com_user_project_foo_bar"
        );
    }

    #[test]
    fn test_cache_manager_paths() {
        let temp_dir = TempDir::new().unwrap();
        let cache = CacheManager::new(temp_dir.path(), None);

        let repo = "https://github.com/org/repo.git";
        let ref_ = "main";

        let ref_path = cache.ref_cache_path(repo, ref_);
        assert!(ref_path.to_string_lossy().contains(".checksy-cache"));
        assert!(ref_path
            .to_string_lossy()
            .contains("https___github.com_org_repo.git"));
        assert!(ref_path.to_string_lossy().contains("main"));

        let config_path = cache.get_config_path(&GitRemote {
            repo: repo.to_string(),
            ref_: ref_.to_string(),
            path: ".checksy.yaml".to_string(),
        });
        assert!(config_path.to_string_lossy().contains(".checksy.yaml"));
    }

    #[test]
    fn test_is_cached() {
        let temp_dir = TempDir::new().unwrap();
        let cache = CacheManager::new(temp_dir.path(), None);

        let repo = "https://github.com/org/repo.git";
        let ref_ = "main";

        // Not cached initially
        assert!(!cache.is_cached(repo, ref_));

        // Create fake cache structure
        let ref_path = cache.ref_cache_path(repo, ref_);
        fs::create_dir_all(&ref_path).unwrap();
        fs::create_dir(ref_path.join(".git")).unwrap();

        // Now it should be cached
        assert!(cache.is_cached(repo, ref_));
    }

    #[test]
    fn test_prune() {
        let temp_dir = TempDir::new().unwrap();
        let cache = CacheManager::new(temp_dir.path(), None);

        let repo = "https://github.com/org/repo.git";

        // Create cached refs
        let main_path = cache.ref_cache_path(repo, "main");
        fs::create_dir_all(&main_path).unwrap();
        fs::create_dir(main_path.join(".git")).unwrap();

        let develop_path = cache.ref_cache_path(repo, "develop");
        fs::create_dir_all(&develop_path).unwrap();
        fs::create_dir(develop_path.join(".git")).unwrap();

        // Only keep "main", prune "develop"
        let mut used = HashSet::new();
        used.insert((CacheManager::encode_repo_name(repo), "main".to_string()));

        cache.prune(&used).unwrap();

        // Main should still exist
        assert!(main_path.exists());
        // Develop should be removed
        assert!(!develop_path.exists());
    }
}
