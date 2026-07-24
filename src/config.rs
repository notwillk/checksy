use crate::cache::{CacheManager, GitRemote};
use crate::schema::{Config, Rule, Severity};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefinitionOrigin {
    pub(crate) config_path: PathBuf,
    pub(crate) base_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedRule {
    pub(crate) rule: Rule,
    pub(crate) origin: DefinitionOrigin,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedPatternGroup {
    pub(crate) patterns: Vec<String>,
    pub(crate) origin: DefinitionOrigin,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedDefinition {
    pub(crate) root_origin: DefinitionOrigin,
    pub(crate) cache_path: Option<String>,
    pub(crate) check_severity: Option<Severity>,
    pub(crate) fail_severity: Option<Severity>,
    pub(crate) preconditions: Vec<ResolvedRule>,
    pub(crate) rules: Vec<ResolvedRule>,
    pub(crate) pattern_groups: Vec<ResolvedPatternGroup>,
}

impl ResolvedDefinition {
    fn into_config(self) -> Config {
        let root_patterns = self
            .pattern_groups
            .into_iter()
            .find(|group| group.origin == self.root_origin)
            .map(|group| group.patterns)
            .unwrap_or_default();
        Config {
            cache_path: self.cache_path,
            check_severity: self.check_severity,
            fail_severity: self.fail_severity,
            preconditions: self
                .preconditions
                .into_iter()
                .map(|resolved| resolved.rule)
                .collect(),
            rules: self
                .rules
                .into_iter()
                .map(|resolved| resolved.rule)
                .collect(),
            patterns: root_patterns,
        }
    }
}

struct ResolvedFragment {
    cache_path: Option<String>,
    check_severity: Option<Severity>,
    fail_severity: Option<Severity>,
    preconditions: Vec<ResolvedRule>,
    rules: Vec<ResolvedRule>,
    origin: DefinitionOrigin,
}

#[derive(Default)]
struct ResolutionState {
    active: Vec<PathBuf>,
    completed: HashSet<PathBuf>,
    pattern_groups: Vec<ResolvedPatternGroup>,
    display_root: Option<PathBuf>,
}

#[derive(Clone, Copy)]
enum ResolutionMode {
    CanonicalOrigins,
    LegacyPublicPaths,
}

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
    load_resolved_with_mode(path, ResolutionMode::LegacyPublicPaths)
        .map(ResolvedDefinition::into_config)
}

pub(crate) fn load_resolved(path: &str) -> Result<ResolvedDefinition, String> {
    load_resolved_with_mode(path, ResolutionMode::CanonicalOrigins)
}

fn load_resolved_with_mode(path: &str, mode: ResolutionMode) -> Result<ResolvedDefinition, String> {
    if path == "-" {
        let mut stdin = std::io::stdin();
        let mut buffer = String::new();
        stdin
            .read_to_string(&mut buffer)
            .map_err(|error| format!("read stdin: {error}"))?;
        let base_dir = std::env::current_dir()
            .and_then(|path| path.canonicalize())
            .map_err(|error| format!("resolve current directory: {error}"))?;
        return resolve_stdin(&buffer, base_dir);
    }

    let mut state = ResolutionState::default();
    let fragment = resolve_file(Path::new(path), None, mode, &mut state)?
        .expect("the root definition cannot already be completed");

    Ok(ResolvedDefinition {
        root_origin: fragment.origin,
        cache_path: fragment.cache_path,
        check_severity: fragment.check_severity,
        fail_severity: fragment.fail_severity,
        preconditions: fragment.preconditions,
        rules: fragment.rules,
        pattern_groups: state.pattern_groups,
    })
}

pub(crate) fn decode_config(data: &str) -> Result<Config, String> {
    // Keep the generic YAML parse as the authority for YAML-format errors such
    // as duplicate mapping keys and multiple documents. The typed pass then
    // enforces Checksy's closed configuration model.
    serde_yaml::from_str::<serde_yaml::Value>(data)
        .map_err(|error| format!("decode config YAML: {}", error))?;

    serde_yaml::from_str(data).map_err(|error| format!("decode config: {}", error))
}

fn resolve_stdin(data: &str, base_dir: PathBuf) -> Result<ResolvedDefinition, String> {
    let mut config = decode_config(data)?;
    if config
        .preconditions
        .iter()
        .chain(config.rules.iter())
        .any(Rule::is_remote)
    {
        return Err(
            "stdin configuration must be self-contained; `remote` includes are not supported"
                .to_string(),
        );
    }

    apply_rule_defaults(&mut config);
    let origin = DefinitionOrigin {
        config_path: PathBuf::from("<stdin>"),
        base_dir,
    };
    let preconditions = config
        .preconditions
        .into_iter()
        .map(|rule| ResolvedRule {
            rule,
            origin: origin.clone(),
        })
        .collect();
    let rules = config
        .rules
        .into_iter()
        .map(|rule| ResolvedRule {
            rule,
            origin: origin.clone(),
        })
        .collect();
    let pattern_groups = (!config.patterns.is_empty())
        .then(|| ResolvedPatternGroup {
            patterns: config.patterns,
            origin: origin.clone(),
        })
        .into_iter()
        .collect();

    Ok(ResolvedDefinition {
        root_origin: origin,
        cache_path: config.cache_path,
        check_severity: config.check_severity,
        fail_severity: config.fail_severity,
        preconditions,
        rules,
        pattern_groups,
    })
}

fn resolve_file(
    path: &Path,
    parent_defaults: Option<(Option<Severity>, Option<Severity>)>,
    mode: ResolutionMode,
    state: &mut ResolutionState,
) -> Result<Option<ResolvedFragment>, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("read config: {error}"))?;
    if state.display_root.is_none() {
        state.display_root = canonical.parent().map(Path::to_path_buf);
    }

    if let Some(cycle_start) = state.active.iter().position(|active| active == &canonical) {
        let display_chain = state.active[cycle_start..]
            .iter()
            .chain(std::iter::once(&canonical))
            .map(|path| state.display_path(path))
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(format!("local include cycle detected: {display_chain}"));
    }

    if state.completed.contains(&canonical) {
        return Ok(None);
    }

    let data = fs::read_to_string(&canonical).map_err(|error| format!("read config: {error}"))?;
    let mut config = decode_config(&data)?;

    if let Some((check_sev, fail_sev)) = parent_defaults {
        if config.check_severity.is_none() {
            config.check_severity = check_sev;
        }
        if config.fail_severity.is_none() {
            config.fail_severity = fail_sev;
        }
    }

    apply_rule_defaults(&mut config);
    let base_dir_source = match mode {
        ResolutionMode::CanonicalOrigins => canonical.as_path(),
        ResolutionMode::LegacyPublicPaths => path,
    };
    let base_dir = base_dir_source
        .parent()
        .ok_or_else(|| {
            format!(
                "resolve config directory: '{}' has no parent",
                base_dir_source.display()
            )
        })?
        .to_path_buf();
    let origin = DefinitionOrigin {
        config_path: canonical.clone(),
        base_dir: base_dir.clone(),
    };
    if !config.patterns.is_empty() {
        state.pattern_groups.push(ResolvedPatternGroup {
            patterns: std::mem::take(&mut config.patterns),
            origin: origin.clone(),
        });
    }

    state.active.push(canonical.clone());
    let result = (|| {
        let parent_defaults = (config.check_severity, config.fail_severity);
        let preconditions = resolve_rule_list(
            config.preconditions,
            &base_dir,
            config.cache_path.as_deref(),
            parent_defaults,
            &origin,
            mode,
            state,
        )?;
        let rules = resolve_rule_list(
            config.rules,
            &base_dir,
            config.cache_path.as_deref(),
            parent_defaults,
            &origin,
            mode,
            state,
        )?;

        Ok(ResolvedFragment {
            cache_path: config.cache_path,
            check_severity: config.check_severity,
            fail_severity: config.fail_severity,
            preconditions,
            rules,
            origin,
        })
    })();
    let popped = state.active.pop();
    debug_assert_eq!(popped.as_ref(), Some(&canonical));
    if result.is_ok() {
        state.completed.insert(canonical);
    }

    result.map(Some)
}

fn resolve_rule_list(
    rules: Vec<Rule>,
    config_dir: &Path,
    cache_path: Option<&str>,
    parent_defaults: (Option<Severity>, Option<Severity>),
    origin: &DefinitionOrigin,
    mode: ResolutionMode,
    state: &mut ResolutionState,
) -> Result<Vec<ResolvedRule>, String> {
    let mut resolved_rules = Vec::new();
    for rule in rules {
        if !rule.is_remote() {
            resolved_rules.push(ResolvedRule {
                rule,
                origin: origin.clone(),
            });
            continue;
        }

        if let Some(error) = rule.validate_remote_only() {
            return Err(format!(
                "invalid remote rule (remote: {:?}): {}",
                rule.remote, error
            ));
        }
        let remote_path = rule
            .remote
            .as_deref()
            .expect("an include rule has a remote path");
        let resolved_path = resolve_remote_path(config_dir, cache_path, remote_path)?;
        let Some(child) = resolve_file(&resolved_path, Some(parent_defaults), mode, state)? else {
            continue;
        };

        resolved_rules.extend(child.preconditions);
        resolved_rules.extend(child.rules);
    }
    Ok(resolved_rules)
}

impl ResolutionState {
    fn display_path(&self, path: &Path) -> String {
        let display = self
            .display_root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .filter(|relative| !relative.as_os_str().is_empty())
            .unwrap_or(path);
        display.to_string_lossy().replace('\\', "/")
    }
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
                    skip_if: None,
                    severity: None,
                    fix: None,
                    interactive_fix: None,
                    hint: None,
                    remote: None,
                    timeout: None,
                },
                Rule {
                    name: None,
                    check: Some("echo warn".to_string()),
                    skip_if: None,
                    severity: Some(Severity::Warning),
                    fix: None,
                    interactive_fix: None,
                    hint: None,
                    remote: None,
                    timeout: None,
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
    fn test_circular_reference_reports_the_active_chain() {
        let dir = TempDir::new().unwrap();

        let path_a = dir.path().join("a.yaml");
        fs::write(&path_a, "rules:\n  - remote: b.yaml\n").unwrap();
        let path_b = dir.path().join("b.yaml");
        fs::write(&path_b, "rules:\n  - remote: c.yaml\n").unwrap();
        let path_c = dir.path().join("c.yaml");
        fs::write(&path_c, "rules:\n  - remote: a.yaml\n").unwrap();

        let error = load(path_a.to_str().unwrap()).unwrap_err();
        assert_eq!(
            error,
            "local include cycle detected: a.yaml -> b.yaml -> c.yaml -> a.yaml"
        );
    }

    #[test]
    fn circular_reference_chain_disambiguates_repeated_basenames() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("child")).unwrap();

        let root = dir.path().join("config.yaml");
        fs::write(&root, "rules:\n  - remote: child/config.yaml\n").unwrap();
        fs::write(
            dir.path().join("child/config.yaml"),
            "rules:\n  - remote: ../config.yaml\n",
        )
        .unwrap();

        let error = load(root.to_str().unwrap()).unwrap_err();
        assert_eq!(
            error,
            "local include cycle detected: config.yaml -> child/config.yaml -> config.yaml"
        );
    }

    #[cfg(unix)]
    #[test]
    fn public_load_preserves_legacy_symlink_relative_include_behavior() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target");
        let alias = dir.path().join("alias");
        fs::create_dir(&target).unwrap();
        fs::create_dir(&alias).unwrap();
        fs::write(target.join("root.yaml"), "rules:\n  - remote: child.yaml\n").unwrap();
        fs::write(
            target.join("child.yaml"),
            "rules:\n  - name: target child\n    check: 'true'\n",
        )
        .unwrap();
        fs::write(
            alias.join("child.yaml"),
            "rules:\n  - name: alias child\n    check: 'true'\n",
        )
        .unwrap();
        let alias_root = alias.join("root.yaml");
        symlink(target.join("root.yaml"), &alias_root).unwrap();

        let public = load(alias_root.to_str().unwrap()).unwrap();
        assert_eq!(public.rules[0].name.as_deref(), Some("alias child"));

        let resolved = load_resolved(alias_root.to_str().unwrap()).unwrap();
        assert_eq!(resolved.rules[0].rule.name.as_deref(), Some("target child"));
        assert_eq!(
            resolved.rules[0].origin.base_dir,
            target.canonicalize().unwrap()
        );
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
    fn resolved_definitions_preserve_origins_order_defaults_and_pattern_groups() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("nested");
        let deeper = nested.join("deeper");
        fs::create_dir_all(&deeper).unwrap();

        fs::write(
            deeper.join("c.yaml"),
            "\
checkSeverity: info
patterns:
  - c/*.sh
preconditions:
  - name: c-pre
    check: echo c-pre
rules:
  - name: c-rule
    check: echo c-rule
",
        )
        .unwrap();
        fs::write(
            nested.join("a.yaml"),
            "\
patterns:
  - a/*.sh
preconditions:
  - name: a-pre
    check: echo a-pre
  - remote: deeper/c.yaml
rules:
  - name: a-rule
    check: echo a-rule
",
        )
        .unwrap();
        fs::write(
            nested.join("b.yaml"),
            "\
patterns:
  - b/*.sh
preconditions:
  - name: b-pre
    check: echo b-pre
rules:
  - name: b-rule
    check: echo b-rule
    severity: debug
",
        )
        .unwrap();
        let root = dir.path().join("main.yaml");
        fs::write(
            &root,
            "\
checkSeverity: warn
patterns:
  - root/*.sh
preconditions:
  - name: root-pre
    check: echo root-pre
  - remote: nested/a.yaml
rules:
  - name: root-rule
    check: echo root-rule
  - remote: nested/b.yaml
",
        )
        .unwrap();

        let resolved = load_resolved(root.to_str().unwrap()).unwrap();

        assert_eq!(
            resolved.root_origin.config_path,
            root.canonicalize().unwrap()
        );
        assert_eq!(
            resolved.root_origin.base_dir,
            dir.path().canonicalize().unwrap()
        );
        assert_eq!(
            resolved
                .preconditions
                .iter()
                .map(|resolved| resolved.rule.name.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["root-pre", "a-pre", "c-pre", "c-rule", "a-rule"]
        );
        assert_eq!(
            resolved
                .rules
                .iter()
                .map(|resolved| resolved.rule.name.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["root-rule", "b-pre", "b-rule"]
        );
        assert_eq!(
            resolved
                .pattern_groups
                .iter()
                .map(|group| group.patterns[0].as_str())
                .collect::<Vec<_>>(),
            ["root/*.sh", "a/*.sh", "c/*.sh", "b/*.sh"]
        );

        let canonical_nested = nested.canonicalize().unwrap();
        let canonical_deeper = deeper.canonicalize().unwrap();
        assert_eq!(
            resolved.preconditions[0].origin.base_dir,
            dir.path().canonicalize().unwrap()
        );
        assert_eq!(resolved.preconditions[1].origin.base_dir, canonical_nested);
        assert_eq!(resolved.preconditions[2].origin.base_dir, canonical_deeper);
        assert_eq!(
            resolved
                .preconditions
                .iter()
                .map(|resolved| resolved.rule.severity.unwrap())
                .collect::<Vec<_>>(),
            [
                Severity::Warning,
                Severity::Warning,
                Severity::Info,
                Severity::Info,
                Severity::Warning,
            ]
        );
        assert_eq!(resolved.rules[2].rule.severity, Some(Severity::Debug));

        let flattened = load(root.to_str().unwrap()).unwrap();
        assert_eq!(flattened.patterns, ["root/*.sh"]);
    }

    #[test]
    fn completed_includes_are_deduplicated_after_depth_first_resolution() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("leaf.yaml"),
            "patterns: [leaf/*.sh]\nrules:\n  - name: leaf\n    check: echo leaf\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("child.yaml"),
            "\
patterns: [child/*.sh]
rules:
  - name: child
    check: echo child
  - remote: leaf.yaml
",
        )
        .unwrap();
        let root = dir.path().join("root.yaml");
        fs::write(
            &root,
            "\
patterns: [root/*.sh]
rules:
  - remote: child.yaml
  - remote: leaf.yaml
  - remote: child.yaml
",
        )
        .unwrap();

        let resolved = load_resolved(root.to_str().unwrap()).unwrap();
        assert_eq!(
            resolved
                .rules
                .iter()
                .map(|resolved| resolved.rule.name.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["child", "leaf"]
        );
        assert_eq!(
            resolved
                .pattern_groups
                .iter()
                .map(|group| group.patterns[0].as_str())
                .collect::<Vec<_>>(),
            ["root/*.sh", "child/*.sh", "leaf/*.sh"]
        );
    }

    #[test]
    fn stdin_resolution_is_self_contained_and_uses_the_callers_directory() {
        let dir = TempDir::new().unwrap();
        let base_dir = dir.path().canonicalize().unwrap();
        let resolved = resolve_stdin(
            "patterns: [scripts/*.sh]\nrules:\n  - check: pwd\n",
            base_dir.clone(),
        )
        .unwrap();

        assert_eq!(resolved.root_origin.config_path, Path::new("<stdin>"));
        assert_eq!(resolved.root_origin.base_dir, base_dir);
        assert_eq!(resolved.rules[0].origin, resolved.root_origin);
        assert_eq!(resolved.pattern_groups[0].origin, resolved.root_origin);

        let error = resolve_stdin("rules:\n  - remote: child.yaml\n", dir.path().to_path_buf())
            .unwrap_err();
        assert_eq!(
            error,
            "stdin configuration must be self-contained; `remote` includes are not supported"
        );
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
