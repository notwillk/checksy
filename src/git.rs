use std::path::Path;
use std::process::Command;

/// Cache git remotes using shallow clones via git CLI
pub struct GitCache;

impl GitCache {
    /// Perform a shallow clone of a specific ref from a remote repository
    ///
    /// This clones with --depth 1 and --branch <ref> to get a single-commit shallow clone
    /// of just the specified ref.
    ///
    /// # Arguments
    /// * `repo_url` - The git repository URL (e.g., "https://github.com/user/repo.git")
    /// * `ref_` - The branch/tag/ref to clone (e.g., "main", "v1.0.0")
    /// * `dest` - Destination directory for the clone
    ///
    /// # Returns
    /// * `Ok(())` if clone succeeds
    /// * `Err(String)` with error message if clone fails
    pub fn shallow_clone(repo_url: &str, ref_: &str, dest: &Path) -> Result<(), String> {
        // Ensure parent directory exists
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create cache directory: {}", e))?;
        }

        // Check if git is available
        match Command::new("git").arg("--version").output() {
            Ok(_) => (),
            Err(_) => return Err("git command not found. Please install git".to_string()),
        }

        // Perform shallow clone
        // git clone --depth 1 --branch <ref> <url> <dest>
        let output = Command::new("git")
            .arg("clone")
            .arg("--depth")
            .arg("1")
            .arg("--branch")
            .arg(ref_)
            .arg(repo_url)
            .arg(dest)
            .output()
            .map_err(|e| format!("failed to execute git clone: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "git clone failed for {} (ref: {}): {}",
                repo_url, ref_, stderr
            ));
        }

        Ok(())
    }

    /// Ensure a git remote is cached
    /// If already cached, returns Ok immediately
    /// Otherwise performs a shallow clone
    ///
    /// # Arguments
    /// * `cache_root` - Root of the git cache directory
    /// * `repo_url` - The git repository URL
    /// * `ref_` - The branch/tag/ref
    pub fn ensure_cached(cache_root: &Path, repo_url: &str, ref_: &str) -> Result<(), String> {
        use crate::cache::CacheManager;

        let cache_mgr = CacheManager::new(cache_root.parent().unwrap_or(cache_root), None);

        // Check if already cached
        if cache_mgr.is_cached(repo_url, ref_) {
            return Ok(());
        }

        // Get the destination path
        let dest = cache_mgr.ref_cache_path(repo_url, ref_);

        // Perform shallow clone
        Self::shallow_clone(repo_url, ref_, &dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Note: These tests would require network access to real git repos
    // They're marked as ignored by default for offline development

    #[test]
    #[ignore = "requires network access and git installed"]
    fn test_shallow_clone_public_repo() {
        let temp_dir = TempDir::new().unwrap();
        let dest = temp_dir.path().join("clone");

        // Clone a small public repo (checksy itself)
        let result =
            GitCache::shallow_clone("https://github.com/notwillk/checksy.git", "main", &dest);

        assert!(result.is_ok(), "Clone failed: {:?}", result.err());
        assert!(dest.join(".git").exists());
        assert!(dest.join("README.md").exists());
    }

    #[test]
    #[ignore = "requires network access and git installed"]
    fn test_shallow_clone_invalid_repo() {
        let temp_dir = TempDir::new().unwrap();
        let dest = temp_dir.path().join("clone");

        // Try to clone non-existent repo
        let result = GitCache::shallow_clone(
            "https://github.com/nonexistent-user-12345/fake-repo.git",
            "main",
            &dest,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed"));
    }
}
