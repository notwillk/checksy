use crate::cache::{CacheManager, GitRemote};
use crate::schema::{Config, Severity};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn resolve_path(explicit: &str) -> Result<Option<String>, String> {
    if explicit == "-" {
        return Ok(Some("-".to_string()));
    }

    if !explicit.is_empty() {
        let path = Path::new(explicit);
        if !path.exists() {
            return Err(format!("config file {} does not exist", explicit));
        }
        if path.is_dir() {
            return Err(format!("config file {} is a directory", explicit));
        }
        return Ok(Some(explicit.to_string()));
    }

    for candidate in &[".checksy.yaml", ".checksy.yml"] {
        let path = Path::new(candidate);
        if path.exists() {
            if path.is_dir() {
                return Err(format!("config file {} is a directory", candidate));
            }
            return Ok(Some(candidate.to_string()));
        }
    }

    Ok(None)
}

pub fn load(path: &str) -> Result<Config, String> {
    load_with_context(path, None, &mut HashSet::new())
}

fn load_with_context(
    path: &str,
    parent_defaults: Option<(Option<Severity>, Option<Severity>)>,
    visited: &mut HashSet<PathBuf>,
) -> Result<Config, String> {
    // For non-stdin paths, check if already visited BEFORE loading
    // to prevent circular references
    if path != "-" {
        let config_path = Path::new(path);
        if let Ok(canonical) = config_path.canonicalize() {
            if visited.contains(&canonical) {
                // Already visited this config (circular reference)
                // Return empty config with inherited defaults
                return Ok(Config {
                    cache_path: None,
                    check_severity: parent_defaults.and_then(|(s, _)| s),
                    fail_severity: parent_defaults.and_then(|(_, s)| s),
                    preconditions: vec![],
                    rules: vec![],
                    patterns: vec![],
                });
            }
        }
    }

    let data = if path == "-" {
        let mut stdin = std::io::stdin();
        let mut buffer = String::new();
        stdin
            .read_to_string(&mut buffer)
            .map_err(|e| format!("read stdin: {}", e))?;
        buffer
    } else {
        fs::read_to_string(path).map_err(|e| format!("read config: {}", e))?
    };

    let json_data = serde_yaml::from_str::<serde_yaml::Value>(&data)
        .map_err(|e| format!("decode config YAML: {}", e))?;

    let _json_str =
        serde_json::to_string(&json_data).map_err(|e| format!("convert config to JSON: {}", e))?;

    let mut cfg: Config =
        serde_yaml::from_str(&data).map_err(|e| format!("decode config: {}", e))?;

    // Apply inherited defaults if parent provided them
    if let Some((check_sev, fail_sev)) = parent_defaults {
        if cfg.check_severity.is_none() {
            cfg.check_severity = check_sev;
        }
        if cfg.fail_severity.is_none() {
            cfg.fail_severity = fail_sev;
        }
    }

    apply_rule_defaults(&mut cfg);

    // Expand remote configs if this is not stdin
    if path != "-" {
        let config_path = Path::new(path);
        let config_dir = config_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        // Mark this config as visited before expanding remotes
        if let Ok(canonical) = config_path.canonicalize() {
            visited.insert(canonical);
        }

        expand_remotes(&mut cfg, &config_dir, visited)?;
    }

    Ok(cfg)
}

fn expand_remotes(
    cfg: &mut Config,
    config_dir: &Path,
    visited: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    // Expand preconditions first
    let mut expanded_preconditions = Vec::new();
    for rule in cfg.preconditions.drain(..) {
        if rule.is_remote() {
            // Validate that remote rule has no other properties
            if let Some(error) = rule.validate_remote_only() {
                return Err(format!(
                    "invalid remote rule (remote: {:?}): {}",
                    rule.remote, error
                ));
            }

            let remote_path = rule.remote.as_ref().unwrap();
            let resolved = resolve_remote_path(config_dir, cfg.cache_path.as_deref(), remote_path)?;

            // Load and expand the remote config
            // (load_with_context will handle circular reference detection)
            let parent_defaults = (cfg.check_severity, cfg.fail_severity);
            let remote_cfg = load_with_context(
                resolved.to_string_lossy().as_ref(),
                Some(parent_defaults),
                visited,
            )?;

            // Add all remote rules to expanded preconditions
            expanded_preconditions.extend(remote_cfg.preconditions);
            expanded_preconditions.extend(remote_cfg.rules);
        } else {
            expanded_preconditions.push(rule);
        }
    }
    cfg.preconditions = expanded_preconditions;

    // Expand main rules
    let mut expanded_rules = Vec::new();
    for rule in cfg.rules.drain(..) {
        if rule.is_remote() {
            // Validate that remote rule has no other properties
            if let Some(error) = rule.validate_remote_only() {
                return Err(format!(
                    "invalid remote rule (remote: {:?}): {}",
                    rule.remote, error
                ));
            }

            let remote_path = rule.remote.as_ref().unwrap();
            let resolved = resolve_remote_path(config_dir, cfg.cache_path.as_deref(), remote_path)?;

            // Load and expand the remote config
            // (load_with_context will handle circular reference detection)
            let parent_defaults = (cfg.check_severity, cfg.fail_severity);
            let remote_cfg = load_with_context(
                resolved.to_string_lossy().as_ref(),
                Some(parent_defaults),
                visited,
            )?;

            // Add all remote rules to expanded rules
            expanded_rules.extend(remote_cfg.preconditions);
            expanded_rules.extend(remote_cfg.rules);
        } else {
            expanded_rules.push(rule);
        }
    }
    cfg.rules = expanded_rules;

    Ok(())
}

/// Parse a remote string to detect git-based resource locators
/// Format: git+<repo>#<ref>:<path>
///   - ref defaults to "main"
///   - path defaults to ".checksy.yaml"
/// Returns None for regular file paths
/// Returns Some(GitRemote) for git-based locators
pub fn parse_git_remote(remote_str: &str) -> Option<GitRemote> {
    if !remote_str.starts_with("git+") {
        return None;
    }

    // Remove the "git+" prefix
    let rest = &remote_str[4..];

    // First, check for # to split repo from ref+path
    let (repo_part, after_repo) = if let Some(hash_pos) = rest.find('#') {
        (&rest[..hash_pos], &rest[hash_pos + 1..])
    } else {
        // No # found, entire string is repo, use defaults for ref and path
        return Some(GitRemote {
            repo: rest.to_string(),
            ref_: "main".to_string(),
            path: ".checksy.yaml".to_string(),
        });
    };

    // Now parse ref:path from after_repo
    let (ref_part, path_part) = if let Some(colon_pos) = after_repo.find(':') {
        (&after_repo[..colon_pos], &after_repo[colon_pos + 1..])
    } else {
        // No : found, after_repo is just the ref, use default path
        (after_repo, ".checksy.yaml")
    };

    let repo = repo_part.to_string();
    let ref_ = if ref_part.is_empty() {
        "main".to_string()
    } else {
        ref_part.to_string()
    };
    let path = if path_part.is_empty() {
        ".checksy.yaml".to_string()
    } else {
        path_part.to_string()
    };

    Some(GitRemote { repo, ref_, path })
}

/// Resolves a remote config path
/// For git remotes, checks if the remote is cached and returns the cached path
pub fn resolve_remote_path(
    config_dir: &Path,
    cache_path: Option<&str>,
    remote_path: &str,
) -> Result<PathBuf, String> {
    // Check for git-based resource locator
    if let Some(git_remote) = parse_git_remote(remote_path) {
        // Check if this git remote is cached
        let cache_mgr = CacheManager::new(config_dir, cache_path);

        if !cache_mgr.is_cached(&git_remote.repo, &git_remote.ref_) {
            return Err(format!(
                "git remote not cached: {}. Run 'checksy install' first",
                remote_path
            ));
        }

        let config_path = cache_mgr.get_config_path(&git_remote);

        if !config_path.exists() {
            return Err(format!(
                "cached config not found: {} (expected at: {})",
                git_remote.path,
                config_path.display()
            ));
        }

        return Ok(config_path);
    }

    let path = config_dir.join(remote_path);

    if !path.exists() {
        return Err(format!(
            "remote config '{}' not found (resolved to: {})",
            remote_path,
            path.display()
        ));
    }

    if path.is_dir() {
        return Err(format!("remote config '{}' is a directory", remote_path));
    }

    // Get canonical path to ensure consistent tracking
    path.canonicalize()
        .map_err(|e| format!("failed to resolve remote path '{}': {}", remote_path, e))
}

fn apply_rule_defaults(cfg: &mut Config) {
    // Determine default severity from config or fall back to Error
    let default_severity = cfg.check_severity.unwrap_or(Severity::Error);

    // Don't apply defaults to remote rules - they will be replaced during expansion
    // and defaults will be applied to the expanded rules
    for rule in &mut cfg.rules {
        if !rule.is_remote() && rule.severity.is_none() {
            rule.severity = Some(default_severity);
        }
    }

    for rule in &mut cfg.preconditions {
        if !rule.is_remote() && rule.severity.is_none() {
            rule.severity = Some(default_severity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Rule;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_path_explicit() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cfg.yaml");
        fs::write(&path, "rules: []").unwrap();

        let got = resolve_path(path.to_str().unwrap());
        assert!(got.is_ok());
        assert!(got.unwrap().is_some());
    }

    #[test]
    fn test_resolve_path_auto_detect() {
        let dir = TempDir::new().unwrap();
        let old_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        fs::write(
            dir.path().join(".checksy.yaml"),
            "rules:\n  - check: echo ok\n",
        )
        .unwrap();

        let got = resolve_path("");
        std::env::set_current_dir(old_cwd).unwrap();

        assert!(got.is_ok());
        assert_eq!(got.unwrap(), Some(".checksy.yaml".to_string()));
    }

    #[test]
    fn test_apply_rule_defaults() {
        let mut cfg = Config {
            cache_path: None,
            check_severity: None,
            fail_severity: None,
            preconditions: vec![],
            rules: vec![
                Rule {
                    name: None,
                    check: Some("echo hi".to_string()),
                    severity: None,
                    fix: None,
                    hint: None,
                    remote: None,
                },
                Rule {
                    name: None,
                    check: Some("echo warn".to_string()),
                    severity: Some(Severity::Warning),
                    fix: None,
                    hint: None,
                    remote: None,
                },
            ],
            patterns: vec![],
        };

        apply_rule_defaults(&mut cfg);

        assert_eq!(cfg.rules[0].severity, Some(Severity::Error));
        assert_eq!(cfg.rules[1].severity, Some(Severity::Warning));
    }

    #[test]
    fn test_load_applies_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(&path, "rules:\n  - name: warn\n    check: echo warn\n    severity: warn\n  - name: default\n    check: echo ok\n").unwrap();

        let result = load(path.to_str().unwrap());
        if let Err(e) = &result {
            eprintln!("Load error: {}", e);
        }
        assert!(result.is_ok(), "Failed to load config");

        let cfg = result.unwrap();
        assert_eq!(cfg.rules[0].severity, Some(Severity::Warning));
        assert_eq!(cfg.rules[1].severity, Some(Severity::Error));
    }

    #[test]
    fn test_load_patterns() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            "rules: []\npatterns:\n  - 'tests/*.sh'\n  - '!tests/skip.sh'\n",
        )
        .unwrap();

        let cfg = load(path.to_str().unwrap());
        assert!(cfg.is_ok());

        let cfg = cfg.unwrap();
        assert_eq!(cfg.patterns.len(), 2);
        assert_eq!(cfg.patterns[0], "tests/*.sh");
    }

    #[test]
    fn test_load_remote_config() {
        let dir = TempDir::new().unwrap();

        // Create remote config
        let remote_path = dir.path().join("remote.yaml");
        fs::write(
            &remote_path,
            "rules:\n  - name: remote_rule\n    check: echo remote\n    severity: warn\n",
        )
        .unwrap();

        // Create main config with remote reference
        let main_path = dir.path().join("main.yaml");
        fs::write(&main_path, "rules:\n  - remote: remote.yaml\n").unwrap();

        let result = load(main_path.to_str().unwrap());
        assert!(result.is_ok(), "Failed to load config: {:?}", result.err());

        let cfg = result.unwrap();
        assert_eq!(cfg.rules.len(), 1);
        assert_eq!(cfg.rules[0].name, Some("remote_rule".to_string()));
        assert_eq!(cfg.rules[0].check, Some("echo remote".to_string()));
        assert_eq!(cfg.rules[0].severity, Some(Severity::Warning));
    }

    #[test]
    fn test_remote_rule_validation_fails_with_extra_props() {
        let dir = TempDir::new().unwrap();

        // Create main config with invalid remote rule
        let main_path = dir.path().join("main.yaml");
        fs::write(
            &main_path,
            "rules:\n  - remote: somewhere.yaml\n    check: echo bad\n",
        )
        .unwrap();

        let result = load(main_path.to_str().unwrap());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("remote rule cannot have properties"));
    }

    #[test]
    fn test_circular_reference_handled() {
        let dir = TempDir::new().unwrap();

        // Create config A that references B
        let path_a = dir.path().join("a.yaml");
        fs::write(
            &path_a,
            "rules:\n  - name: rule_a\n    check: echo A\n  - remote: b.yaml\n",
        )
        .unwrap();

        // Create config B that references A (circular)
        let path_b = dir.path().join("b.yaml");
        fs::write(
            &path_b,
            "rules:\n  - name: rule_b\n    check: echo B\n  - remote: a.yaml\n",
        )
        .unwrap();

        let result = load(path_a.to_str().unwrap());
        assert!(result.is_ok(), "Failed with circular: {:?}", result.err());

        let cfg = result.unwrap();
        // Should have rule_a, rule_b (A's rule + B's rule, but not duplicate from circular)
        assert_eq!(cfg.rules.len(), 2);
    }

    #[test]
    fn test_remote_inherits_parent_defaults() {
        let dir = TempDir::new().unwrap();

        // Create remote config without severity default
        let remote_path = dir.path().join("remote.yaml");
        fs::write(
            &remote_path,
            "rules:\n  - name: remote_rule\n    check: echo remote\n",
        )
        .unwrap();

        // Create main config with check_severity set
        let main_path = dir.path().join("main.yaml");
        fs::write(
            &main_path,
            "checkSeverity: warn\nrules:\n  - remote: remote.yaml\n",
        )
        .unwrap();

        let result = load(main_path.to_str().unwrap());
        assert!(result.is_ok(), "Failed: {:?}", result.err());

        let cfg = result.unwrap();
        assert_eq!(cfg.rules[0].severity, Some(Severity::Warning));
    }

    #[test]
    fn test_parse_git_remote_basic() {
        let result = parse_git_remote("git+https://github.com/user/repo.git");
        assert!(result.is_some());
        let git = result.unwrap();
        assert_eq!(git.repo, "https://github.com/user/repo.git");
        assert_eq!(git.ref_, "main");
        assert_eq!(git.path, ".checksy.yaml");
    }

    #[test]
    fn test_parse_git_remote_with_ref_and_path() {
        let result =
            parse_git_remote("git+https://github.com/user/repo.git#v1.0.0:configs/dev.yaml");
        assert!(result.is_some());
        let git = result.unwrap();
        assert_eq!(git.repo, "https://github.com/user/repo.git");
        assert_eq!(git.ref_, "v1.0.0");
        assert_eq!(git.path, "configs/dev.yaml");
    }

    #[test]
    fn test_parse_git_remote_with_path_only() {
        // Without #ref, the entire string is the repo URL (including colons if present)
        // The :path separator only works after #ref
        let result = parse_git_remote("git+https://github.com/user/repo.git:other.yaml");
        assert!(result.is_some());
        let git = result.unwrap();
        // No # found, so everything after git+ is the repo
        assert_eq!(git.repo, "https://github.com/user/repo.git:other.yaml");
        assert_eq!(git.ref_, "main"); // default
        assert_eq!(git.path, ".checksy.yaml"); // default
    }

    #[test]
    fn test_parse_git_remote_empty_ref_with_path() {
        // Format: git+<repo>#:<path> (empty ref should default to "main")
        let result =
            parse_git_remote("git+git@github.com:notwillk/checks.git#:github.checksy.yaml");
        assert!(result.is_some());
        let git = result.unwrap();
        assert_eq!(git.repo, "git@github.com:notwillk/checks.git");
        assert_eq!(git.ref_, "main"); // empty ref defaults to main
        assert_eq!(git.path, "github.checksy.yaml");
    }

    #[test]
    fn test_parse_git_remote_empty_ref_with_path_https() {
        // Format: git+<repo>#:<path> with HTTPS URL
        let result =
            parse_git_remote("git+https://github.com/notwillk/checksy.git#:configs/dev.yaml");
        assert!(result.is_some());
        let git = result.unwrap();
        assert_eq!(git.repo, "https://github.com/notwillk/checksy.git");
        assert_eq!(git.ref_, "main"); // empty ref defaults to main
        assert_eq!(git.path, "configs/dev.yaml");
    }

    #[test]
    fn test_parse_git_remote_not_matching() {
        // Regular file paths should return None
        assert!(parse_git_remote("shared.yaml").is_none());
        assert!(parse_git_remote("./config.yaml").is_none());
        assert!(parse_git_remote("/absolute/path.yaml").is_none());
        assert!(parse_git_remote("https://example.com/config.yaml").is_none());
    }

    #[test]
    fn test_git_remote_not_implemented_error() {
        let dir = TempDir::new().unwrap();

        let main_path = dir.path().join("main.yaml");
        fs::write(
            &main_path,
            "rules:\n  - remote: git+https://github.com/user/repo.git\n",
        )
        .unwrap();

        let result = load(main_path.to_str().unwrap());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("git remote not cached"));
        assert!(err.contains("Run 'checksy install' first"));
    }
}
