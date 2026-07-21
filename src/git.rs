use crate::process_runner::{
    self, CapturedOutput, CompletedProcess, ProcessError, ProcessLimits, DEFAULT_GIT_FETCH_TIMEOUT,
    DEFAULT_GIT_RESOLVE_TIMEOUT, DEFAULT_TERM_GRACE,
};
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
        let mut version = git_command();
        version.arg("--version");
        match run_git(version, DEFAULT_GIT_RESOLVE_TIMEOUT, "version check") {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                return Err(with_process_output(
                    "git command not found. Please install git".to_string(),
                    &output.stdout,
                    &output.stderr,
                ))
            }
            Err(error) => return Err(error),
        }

        // Perform shallow clone
        // git clone --depth 1 --branch <ref> <url> <dest>
        let command = clone_command(repo_url, ref_, dest);
        let output = run_git(command, DEFAULT_GIT_FETCH_TIMEOUT, "clone")?;

        if !output.status.success() {
            return Err(with_process_output(
                format!("git clone failed for {} (ref: {})", repo_url, ref_),
                &output.stdout,
                &output.stderr,
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
        let dest = cache_mgr.prepare_ref_cache_path(repo_url, ref_)?;

        // Check if already cached
        if dest.join(".git").is_dir() {
            return Ok(());
        }

        // Perform shallow clone
        Self::shallow_clone(repo_url, ref_, &dest)
    }

    /// Get the current SHA of the cached repository
    /// Runs `git rev-parse HEAD` in the cache directory
    ///
    /// # Arguments
    /// * `cache_path` - Path to the cached repository
    ///
    /// # Returns
    /// * `Ok(String)` with the SHA if successful
    /// * `Err(String)` with error message if command fails
    pub fn get_local_sha(cache_path: &Path) -> Result<String, String> {
        let mut command = git_command();
        command
            .arg("-C")
            .arg(cache_path)
            .arg("rev-parse")
            .arg("HEAD");
        let output = run_git(command, DEFAULT_GIT_RESOLVE_TIMEOUT, "rev-parse")?;

        if !output.status.success() {
            return Err(with_process_output(
                "git rev-parse failed".to_string(),
                &output.stdout,
                &output.stderr,
            ));
        }

        let sha = output.stdout.render_lossy();
        Ok(sha.trim().to_string())
    }

    /// Get the remote SHA for a specific ref
    /// Runs `git ls-remote <repo_url> <ref_>`
    ///
    /// # Arguments
    /// * `repo_url` - The git repository URL
    /// * `ref_` - The branch/tag/ref
    ///
    /// # Returns
    /// * `Ok(String)` with the SHA if successful
    /// * `Err(String)` with error message if command fails (network, auth, etc.)
    pub fn get_remote_sha(repo_url: &str, ref_: &str) -> Result<String, String> {
        let command = ls_remote_command(repo_url, ref_);
        let output = run_git(command, DEFAULT_GIT_RESOLVE_TIMEOUT, "ls-remote")?;

        if !output.status.success() {
            return Err(with_process_output(
                "git ls-remote failed".to_string(),
                &output.stdout,
                &output.stderr,
            ));
        }

        // Parse output: first column is SHA, second is ref name
        // Example: "abc123\trefs/heads/main\n"
        let stdout = output.stdout.render_lossy();
        let line = stdout.lines().next().ok_or("empty ls-remote output")?;
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.is_empty() {
            return Err("invalid ls-remote output format".to_string());
        }

        Ok(parts[0].trim().to_string())
    }
}

fn git_command() -> Command {
    #[cfg(target_os = "linux")]
    const ASKPASS_PROGRAM: &str = "/bin/false";
    #[cfg(target_os = "macos")]
    const ASKPASS_PROGRAM: &str = "/usr/bin/false";
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    const ASKPASS_PROGRAM: &str = "false";

    const BATCH_SSH_COMMAND: &str = "ssh -oBatchMode=yes";

    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg("credential.interactive=false")
        .arg("-c")
        .arg(format!("core.askPass={ASKPASS_PROGRAM}"))
        .arg("-c")
        .arg(format!("core.sshCommand={BATCH_SSH_COMMAND}"))
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .env("GIT_ASKPASS", ASKPASS_PROGRAM)
        .env("SSH_ASKPASS", ASKPASS_PROGRAM)
        .env("SSH_ASKPASS_REQUIRE", "never")
        // Environment-level SSH commands override `core.sshCommand`; replace
        // them explicitly so an inherited wrapper cannot re-enable prompts.
        .env("GIT_SSH_COMMAND", BATCH_SSH_COMMAND)
        .env_remove("GIT_SSH")
        .env("GIT_SSH_VARIANT", "ssh")
        // Do not inherit environment-injected command-scope Git config that
        // could countermand the fixed noninteractive profile.
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_CONFIG_COUNT");
    command
}

fn clone_command(repo_url: &str, ref_: &str, dest: &Path) -> Command {
    let mut command = git_command();
    command
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg("--branch")
        .arg(ref_)
        .arg("--")
        .arg(repo_url)
        .arg(dest);
    command
}

fn ls_remote_command(repo_url: &str, ref_: &str) -> Command {
    let mut command = git_command();
    command.arg("ls-remote").arg("--").arg(repo_url).arg(ref_);
    command
}

fn run_git(
    command: Command,
    timeout: std::time::Duration,
    operation: &str,
) -> Result<CompletedProcess, String> {
    process_runner::run(
        command,
        ProcessLimits {
            timeout,
            term_grace: DEFAULT_TERM_GRACE,
        },
    )
    .map_err(|error| {
        let mut message = match &error {
            ProcessError::TimedOut { timeout, .. } => format!(
                "operation-timeout: git {operation} exceeded {}ms",
                timeout.as_millis()
            ),
            other => format!("failed to execute git {operation}: {other}"),
        };
        if let Some(output) = error.output() {
            message = with_process_output(message, &output.stdout, &output.stderr);
        }
        message
    })
}

fn with_process_output(
    mut message: String,
    stdout: &CapturedOutput,
    stderr: &CapturedOutput,
) -> String {
    if stdout.original_bytes != 0 {
        message.push_str("\nstdout:\n");
        message.push_str(&stdout.render_lossy());
    }
    if stderr.original_bytes != 0 {
        message.push_str("\nstderr:\n");
        message.push_str(&stderr.render_lossy());
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use tempfile::TempDir;

    #[test]
    fn git_commands_disable_interactive_credentials_and_passwords() {
        let command = git_command();
        let args: Vec<_> = command.get_args().collect();
        assert!(args.contains(&OsStr::new("credential.interactive=false")));
        #[cfg(target_os = "linux")]
        assert!(args.contains(&OsStr::new("core.askPass=/bin/false")));
        #[cfg(target_os = "macos")]
        assert!(args.contains(&OsStr::new("core.askPass=/usr/bin/false")));
        assert!(args.contains(&OsStr::new("core.sshCommand=ssh -oBatchMode=yes")));

        let environment: std::collections::HashMap<_, _> = command.get_envs().collect();
        assert_eq!(
            environment.get(OsStr::new("GIT_TERMINAL_PROMPT")),
            Some(&Some(OsStr::new("0")))
        );
        assert_eq!(
            environment.get(OsStr::new("GCM_INTERACTIVE")),
            Some(&Some(OsStr::new("Never")))
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            environment.get(OsStr::new("GIT_ASKPASS")),
            Some(&Some(OsStr::new("/bin/false")))
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            environment.get(OsStr::new("GIT_ASKPASS")),
            Some(&Some(OsStr::new("/usr/bin/false")))
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            environment.get(OsStr::new("SSH_ASKPASS")),
            Some(&Some(OsStr::new("/bin/false")))
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            environment.get(OsStr::new("SSH_ASKPASS")),
            Some(&Some(OsStr::new("/usr/bin/false")))
        );
        assert_eq!(
            environment.get(OsStr::new("SSH_ASKPASS_REQUIRE")),
            Some(&Some(OsStr::new("never")))
        );
        assert_eq!(
            environment.get(OsStr::new("GIT_SSH_COMMAND")),
            Some(&Some(OsStr::new("ssh -oBatchMode=yes")))
        );
        assert_eq!(environment.get(OsStr::new("GIT_SSH")), Some(&None));
        assert_eq!(
            environment.get(OsStr::new("GIT_CONFIG_PARAMETERS")),
            Some(&None)
        );
        assert_eq!(environment.get(OsStr::new("GIT_CONFIG_COUNT")), Some(&None));
    }

    #[test]
    fn git_repository_locators_are_separated_from_options() {
        let clone = clone_command("--upload-pack=malicious", "main", Path::new("destination"));
        let clone_args: Vec<_> = clone.get_args().collect();
        let clone_repo = clone_args
            .iter()
            .position(|arg| *arg == OsStr::new("--upload-pack=malicious"))
            .unwrap();
        assert_eq!(clone_args[clone_repo - 1], OsStr::new("--"));

        let ls_remote = ls_remote_command("--upload-pack=malicious", "main");
        let ls_remote_args: Vec<_> = ls_remote.get_args().collect();
        let remote_repo = ls_remote_args
            .iter()
            .position(|arg| *arg == OsStr::new("--upload-pack=malicious"))
            .unwrap();
        assert_eq!(ls_remote_args[remote_repo - 1], OsStr::new("--"));
    }

    #[test]
    fn git_diagnostics_preserve_both_bounded_output_streams() {
        let stdout = CapturedOutput {
            bytes: b"partial stdout".to_vec(),
            original_bytes: 14,
            truncated: false,
        };
        let stderr = CapturedOutput {
            bytes: b"partial stderr".to_vec(),
            original_bytes: 14,
            truncated: false,
        };

        let diagnostic = with_process_output("git failed".to_string(), &stdout, &stderr);
        assert!(diagnostic.contains("stdout:\npartial stdout"));
        assert!(diagnostic.contains("stderr:\npartial stderr"));
    }

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

    #[test]
    #[ignore = "requires network access and git installed"]
    fn test_get_remote_sha_public_repo() {
        // Test against a well-known public repo (checksy itself)
        let result = GitCache::get_remote_sha("https://github.com/notwillk/checksy.git", "main");

        assert!(
            result.is_ok(),
            "Failed to get remote SHA: {:?}",
            result.err()
        );
        let sha = result.unwrap();
        // SHA should be 40 hex characters
        assert_eq!(sha.len(), 40, "SHA should be 40 characters");
        assert!(
            sha.chars().all(|c| c.is_ascii_hexdigit()),
            "SHA should be hex digits"
        );
    }

    #[test]
    fn test_get_remote_sha_invalid_repo() {
        // Test against non-existent repo
        let result = GitCache::get_remote_sha(
            "https://github.com/nonexistent-user-12345/fake-repo.git",
            "main",
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_get_local_sha_not_git_repo() {
        let temp_dir = TempDir::new().unwrap();
        let not_a_repo = temp_dir.path().join("not-a-repo");
        std::fs::create_dir(&not_a_repo).unwrap();

        // Should fail when not a git repository
        let result = GitCache::get_local_sha(&not_a_repo);
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_ensure_cached_rejects_symlinked_cache_ancestor() {
        use std::os::unix::fs::symlink;

        let temp_dir = TempDir::new().unwrap();
        let cache_root = temp_dir.path().join(".checksy-cache");
        let cache = crate::cache::CacheManager::new(temp_dir.path(), None);
        let repo = "https://example.invalid/symlinked.git";
        let slot = cache.ref_cache_path(repo, "main");
        let outside = temp_dir.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::create_dir_all(slot.parent().unwrap().parent().unwrap()).unwrap();
        symlink(&outside, slot.parent().unwrap()).unwrap();

        let error = GitCache::ensure_cached(&cache_root, repo, "main").unwrap_err();
        assert!(error.contains("symbolic link"), "{error}");
        assert!(!outside.join("main").exists());
    }

    #[test]
    #[ignore = "requires network access and git installed"]
    fn test_get_local_sha_after_clone() {
        let temp_dir = TempDir::new().unwrap();
        let dest = temp_dir.path().join("clone");

        // Clone a repo
        GitCache::shallow_clone("https://github.com/notwillk/checksy.git", "main", &dest)
            .expect("Clone should succeed");

        // Get local SHA
        let local_sha = GitCache::get_local_sha(&dest).expect("Should get local SHA");
        assert_eq!(local_sha.len(), 40, "SHA should be 40 characters");

        // Get remote SHA for same ref
        let remote_sha =
            GitCache::get_remote_sha("https://github.com/notwillk/checksy.git", "main")
                .expect("Should get remote SHA");

        // They should match (since we just cloned)
        assert_eq!(local_sha, remote_sha, "SHAs should match after fresh clone");
    }
}
