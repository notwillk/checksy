use crate::cache::{CacheManager, GitRemote};
use crate::git::GitCache;
use crate::resolved::{
    DefinitionKey, DefinitionOrigin, GitDependency, ResolvedDefinition, ResolvedLoad,
    ResolvedPatternGroup, ResolvedRule, ResolverMode, SourceIdentity,
};
use crate::schema::{Config, Severity};
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigDiagnostic {
    source: String,
    field: String,
    value: String,
    canonical: String,
}

impl fmt::Display for ConfigDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "warning: {}: {}: non-lowercase severity '{}' is deprecated; use '{}'",
            self.source, self.field, self.value, self.canonical
        )
    }
}

#[derive(Debug)]
pub(crate) struct LoadedConfig {
    pub(crate) config: Config,
    pub(crate) diagnostics: Vec<ConfigDiagnostic>,
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
    load_with_diagnostics(path).map(|loaded| loaded.config)
}

pub(crate) fn load_with_diagnostics(path: &str) -> Result<LoadedConfig, String> {
    let resolved = load_resolved_with_diagnostics(path)?;
    Ok(LoadedConfig {
        config: resolved.definition.legacy_config(),
        diagnostics: resolved.diagnostics,
    })
}

pub(crate) fn load_resolved_with_diagnostics(path: &str) -> Result<ResolvedLoad, String> {
    load_resolved_with_mode(path, ResolverMode::CachedOnly)
}

pub(crate) fn load_resolved_with_mode(
    path: &str,
    mode: ResolverMode,
) -> Result<ResolvedLoad, String> {
    load_resolved(path, mode, None)
}

/// Discover the next Git dependency frontier for `install`.
///
/// Git definitions that have not yet been refreshed in this invocation are
/// reported but deliberately not decoded. Once a dependency is present in
/// `expanded`, resolution descends into its freshly materialized checkout and
/// can reveal the next nested frontier.
pub(crate) fn load_resolved_for_install(
    path: &str,
    expanded: &HashSet<(String, String)>,
) -> Result<ResolvedLoad, String> {
    load_resolved(path, ResolverMode::RefreshOrClone, Some(expanded))
}

fn load_resolved(
    path: &str,
    mode: ResolverMode,
    install_expanded: Option<&HashSet<(String, String)>>,
) -> Result<ResolvedLoad, String> {
    if path == "-" {
        let mut stdin = std::io::stdin();
        let mut buffer = String::new();
        stdin
            .read_to_string(&mut buffer)
            .map_err(|error| format!("read stdin: {}", error))?;
        return resolve_stdin(&buffer, mode);
    }

    let config_path = Path::new(path)
        .canonicalize()
        .map_err(|error| format!("failed to resolve config path '{}': {}", path, error))?;
    if !config_path.is_file() {
        return Err(format!("config file {} is not a regular file", path));
    }

    let root_dir = config_path
        .parent()
        .ok_or_else(|| {
            format!(
                "config file {} has no parent directory",
                config_path.display()
            )
        })?
        .to_path_buf();
    let root_relative = config_path
        .strip_prefix(&root_dir)
        .map_err(|_| "failed to derive root config path".to_string())?
        .to_path_buf();
    let source_identity = SourceIdentity::Local {
        root: root_dir.clone(),
    };
    let origin = DefinitionOrigin {
        source_identity,
        defining_config_path: config_path.clone(),
        source_relative_config: Some(root_relative),
        base_dir: root_dir.clone(),
        bundle_root: None,
        revision: None,
    };

    // Decode the selected root first: only its cachePath may choose the legacy
    // cache anchor. Nested definitions cannot redirect acquisition.
    let root_loaded = decode_file(&config_path)?;
    let cache = CacheManager::new(&root_dir, root_loaded.config.cache_path.as_deref());
    let mut resolver = DefinitionResolver::new(mode, cache, install_expanded.cloned());
    let definition = resolver.resolve_config(origin, None, Some(root_loaded))?;

    Ok(ResolvedLoad {
        definition,
        diagnostics: resolver.diagnostics,
        git_dependencies: resolver.git_dependencies,
    })
}

pub(crate) fn decode_with_diagnostics(data: &str, source: &str) -> Result<LoadedConfig, String> {
    let yaml_value = serde_yaml::from_str::<serde_yaml::Value>(data)
        .map_err(|error| format!("decode config YAML: {}", error))?;

    let _json = serde_json::to_string(&yaml_value)
        .map_err(|error| format!("convert config to JSON: {}", error))?;

    let config: Config =
        serde_yaml::from_str(data).map_err(|error| format!("decode config: {}", error))?;
    let source = if source == "-" { "<stdin>" } else { source };
    let diagnostics = collect_severity_diagnostics(&yaml_value, source);

    Ok(LoadedConfig {
        config,
        diagnostics,
    })
}

fn decode_file(path: &Path) -> Result<LoadedConfig, String> {
    let data = fs::read_to_string(path)
        .map_err(|error| format!("read config '{}': {}", path.to_string_lossy(), error))?;
    decode_with_diagnostics(&data, &path.to_string_lossy())
}

struct DefinitionResolver {
    mode: ResolverMode,
    cache: CacheManager,
    active: Vec<DefinitionKey>,
    completed: HashSet<DefinitionKey>,
    dependency_keys: HashSet<(String, String)>,
    install_expanded: Option<HashSet<(String, String)>>,
    diagnostics: Vec<ConfigDiagnostic>,
    git_dependencies: Vec<GitDependency>,
}

impl DefinitionResolver {
    fn new(
        mode: ResolverMode,
        cache: CacheManager,
        install_expanded: Option<HashSet<(String, String)>>,
    ) -> Self {
        Self {
            mode,
            cache,
            active: Vec::new(),
            completed: HashSet::new(),
            dependency_keys: HashSet::new(),
            install_expanded,
            diagnostics: Vec::new(),
            git_dependencies: Vec::new(),
        }
    }

    fn resolve_config(
        &mut self,
        origin: DefinitionOrigin,
        parent_defaults: Option<(Option<Severity>, Option<Severity>)>,
        preloaded: Option<LoadedConfig>,
    ) -> Result<ResolvedDefinition, String> {
        let key = DefinitionKey::from(&origin);
        if let Some(cycle_start) = self.active.iter().position(|active| active == &key) {
            let mut chain: Vec<String> = self.active[cycle_start..]
                .iter()
                .map(|active| active.defining_config_path.display().to_string())
                .collect();
            chain.push(key.defining_config_path.display().to_string());
            return Err(format!(
                "circular remote definition reference: {}",
                chain.join(" -> ")
            ));
        }
        if self.completed.contains(&key) {
            return Ok(empty_resolved_definition(parent_defaults));
        }

        self.active.push(key.clone());
        let result = self.resolve_config_inner(origin, parent_defaults, preloaded);
        let popped = self.active.pop();
        debug_assert_eq!(popped.as_ref(), Some(&key));
        if result.is_ok() {
            self.completed.insert(key);
        }
        result
    }

    fn resolve_config_inner(
        &mut self,
        origin: DefinitionOrigin,
        parent_defaults: Option<(Option<Severity>, Option<Severity>)>,
        preloaded: Option<LoadedConfig>,
    ) -> Result<ResolvedDefinition, String> {
        let mut loaded = match preloaded {
            Some(loaded) => loaded,
            None => decode_file(&origin.defining_config_path)?,
        };
        self.diagnostics.append(&mut loaded.diagnostics);

        let cfg = &mut loaded.config;
        if let Some((check_severity, fail_severity)) = parent_defaults {
            if cfg.check_severity.is_none() {
                cfg.check_severity = check_severity;
            }
            if cfg.fail_severity.is_none() {
                cfg.fail_severity = fail_severity;
            }
        }
        apply_rule_defaults(cfg);
        validate_pattern_origins(&cfg.patterns, &origin)?;

        let defaults = (cfg.check_severity, cfg.fail_severity);
        let precondition_rules = std::mem::take(&mut cfg.preconditions);
        let main_rules = std::mem::take(&mut cfg.rules);

        let (preconditions, mut precondition_patterns) =
            self.resolve_section(precondition_rules, &origin, defaults)?;
        let (rules, mut rule_patterns) = self.resolve_section(main_rules, &origin, defaults)?;

        // Pattern execution is a global final phase. Root/current-definition
        // patterns stay ahead of recursively discovered groups, preserving the
        // existing root script order while keeping each group's negations local.
        let mut pattern_groups = vec![ResolvedPatternGroup {
            patterns: std::mem::take(&mut cfg.patterns),
            origin: origin.clone(),
        }];
        pattern_groups.append(&mut precondition_patterns);
        pattern_groups.append(&mut rule_patterns);

        Ok(ResolvedDefinition {
            cache_path: cfg.cache_path.clone(),
            check_severity: cfg.check_severity,
            fail_severity: cfg.fail_severity,
            preconditions,
            rules,
            pattern_groups,
        })
    }

    fn resolve_section(
        &mut self,
        rules: Vec<crate::schema::Rule>,
        origin: &DefinitionOrigin,
        parent_defaults: (Option<Severity>, Option<Severity>),
    ) -> Result<(Vec<ResolvedRule>, Vec<ResolvedPatternGroup>), String> {
        let mut resolved_rules = Vec::new();
        let mut pattern_groups = Vec::new();

        for rule in rules {
            if let Some(remote) = rule.remote.as_deref() {
                if let Some(error) = rule.validate_remote_only() {
                    return Err(format!(
                        "invalid remote rule (remote: {:?}): {}",
                        rule.remote, error
                    ));
                }

                if let Some(remote_definition) =
                    self.resolve_remote(origin, remote, parent_defaults)?
                {
                    resolved_rules.extend(remote_definition.preconditions);
                    resolved_rules.extend(remote_definition.rules);
                    pattern_groups.extend(remote_definition.pattern_groups);
                }
            } else {
                resolved_rules.push(ResolvedRule {
                    rule,
                    origin: origin.clone(),
                });
            }
        }

        Ok((resolved_rules, pattern_groups))
    }

    fn resolve_remote(
        &mut self,
        parent_origin: &DefinitionOrigin,
        remote: &str,
        parent_defaults: (Option<Severity>, Option<Severity>),
    ) -> Result<Option<ResolvedDefinition>, String> {
        if let Some(git_remote) = parse_git_remote(remote) {
            return self.resolve_git_remote(git_remote, remote, parent_defaults);
        }

        let path = Path::new(remote);
        if parent_origin.bundle_root.is_some() && path.is_absolute() {
            return Err(format!(
                "remote config path '{}' in a fetched definition must be relative",
                remote
            ));
        }
        let candidate = parent_origin.base_dir.join(path);
        let canonical = candidate.canonicalize().map_err(|error| {
            format!(
                "remote config '{}' not found (resolved to: {}): {}",
                remote,
                candidate.display(),
                error
            )
        })?;
        if !canonical.is_file() {
            return Err(format!("remote config '{}' is not a regular file", remote));
        }

        if let Some(bundle_root) = &parent_origin.bundle_root {
            ensure_contained(&canonical, bundle_root, "remote config")?;
        }
        let source_relative_config = match &parent_origin.source_identity {
            SourceIdentity::Local { root, .. } => {
                canonical.strip_prefix(root).ok().map(Path::to_path_buf)
            }
            SourceIdentity::Git { .. } => parent_origin
                .bundle_root
                .as_ref()
                .and_then(|root| canonical.strip_prefix(root).ok())
                .map(Path::to_path_buf),
            SourceIdentity::Stdin { .. } => None,
        };
        let base_dir = canonical
            .parent()
            .ok_or_else(|| format!("remote config '{}' has no parent directory", remote))?
            .to_path_buf();
        let origin = DefinitionOrigin {
            source_identity: parent_origin.source_identity.clone(),
            defining_config_path: canonical,
            source_relative_config,
            base_dir,
            bundle_root: parent_origin.bundle_root.clone(),
            revision: parent_origin.revision.clone(),
        };

        self.resolve_config(origin, Some(parent_defaults), None)
            .map(Some)
    }

    fn resolve_git_remote(
        &mut self,
        git_remote: GitRemote,
        display_locator: &str,
        parent_defaults: (Option<Severity>, Option<Severity>),
    ) -> Result<Option<ResolvedDefinition>, String> {
        validate_git_remote(&git_remote)?;
        // Validate the signed/selected entry path before cache lookup or any
        // caller can decide to acquire the repository.
        let relative_config = validate_fetched_relative_path(&git_remote.path)?;
        let cached = self.cache.is_cached(&git_remote.repo, &git_remote.ref_);
        let dependency_key = (git_remote.repo.clone(), git_remote.ref_.clone());
        if self.dependency_keys.insert(dependency_key.clone()) {
            self.git_dependencies.push(GitDependency {
                remote: git_remote.clone(),
                cache_root: self.cache.root().to_path_buf(),
                cached,
            });
        }

        if self
            .install_expanded
            .as_ref()
            .is_some_and(|expanded| !expanded.contains(&dependency_key))
        {
            return Ok(None);
        }

        if !cached {
            return match self.mode {
                ResolverMode::CachedOnly => Err(format!(
                    "git remote not cached: {}. Run 'checksy install' first",
                    display_locator
                )),
                ResolverMode::CacheMissing | ResolverMode::RefreshOrClone => Ok(None),
            };
        }

        let bundle_slot = self
            .cache
            .confined_ref_cache_path(&git_remote.repo, &git_remote.ref_)?;
        let bundle_root = bundle_slot.canonicalize().map_err(|error| {
            format!(
                "failed to resolve cached git bundle for {}#{}: {}",
                git_remote.repo, git_remote.ref_, error
            )
        })?;
        if !bundle_root.is_dir() {
            return Err(format!(
                "cached git bundle is not a directory: {}",
                bundle_root.display()
            ));
        }

        let candidate = bundle_root.join(&relative_config);
        let defining_config_path = candidate.canonicalize().map_err(|error| {
            format!(
                "cached config not found: {} (expected at: {}): {}",
                git_remote.path,
                candidate.display(),
                error
            )
        })?;
        ensure_contained(&defining_config_path, &bundle_root, "cached config")?;
        if !defining_config_path.is_file() {
            return Err(format!(
                "cached config is not a regular file: {}",
                defining_config_path.display()
            ));
        }

        let revision = GitCache::get_local_sha(&bundle_root).map_err(|error| {
            format!(
                "failed to identify cached git bundle {}#{}: {}",
                git_remote.repo, git_remote.ref_, error
            )
        })?;
        let canonical_relative = defining_config_path
            .strip_prefix(&bundle_root)
            .map_err(|_| "cached config escaped its bundle root".to_string())?
            .to_path_buf();
        let base_dir = defining_config_path
            .parent()
            .ok_or_else(|| "cached config has no parent directory".to_string())?
            .to_path_buf();
        let source_identity = SourceIdentity::Git {
            repository: git_remote.repo.clone(),
            selector: git_remote.ref_.clone(),
            checkout: bundle_root.clone(),
        };
        let origin = DefinitionOrigin {
            source_identity,
            defining_config_path,
            source_relative_config: Some(canonical_relative),
            base_dir,
            bundle_root: Some(bundle_root),
            revision: Some(revision),
        };

        self.resolve_config(origin, Some(parent_defaults), None)
            .map(Some)
    }
}

fn empty_resolved_definition(
    parent_defaults: Option<(Option<Severity>, Option<Severity>)>,
) -> ResolvedDefinition {
    let (check_severity, fail_severity) = parent_defaults.unwrap_or((None, None));
    ResolvedDefinition {
        cache_path: None,
        check_severity,
        fail_severity,
        ..ResolvedDefinition::default()
    }
}

fn ensure_contained(path: &Path, root: &Path, description: &str) -> Result<(), String> {
    if path.starts_with(root) {
        Ok(())
    } else {
        Err(format!(
            "{} '{}' escapes source root '{}'",
            description,
            path.display(),
            root.display()
        ))
    }
}

fn validate_git_remote(remote: &GitRemote) -> Result<(), String> {
    let encoded_repo = CacheManager::encode_repo_name(&remote.repo);
    if remote.repo.trim().is_empty()
        || remote.repo.contains('\0')
        || remote.repo.chars().any(char::is_control)
        || matches!(encoded_repo.as_str(), "." | "..")
    {
        return Err(format!("invalid git remote repository '{}'", remote.repo));
    }
    if remote.ref_.trim().is_empty()
        || remote.ref_.contains('\0')
        || remote.ref_.contains('\\')
        || remote
            .ref_
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(format!("invalid git remote ref '{}'", remote.ref_));
    }
    Ok(())
}

fn validate_fetched_relative_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty()
        || value.ends_with('/')
        || value.contains("//")
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        return Err(format!("invalid git config path '{}'", value));
    }

    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "git config path '{}' must be a canonical relative path",
            value
        ));
    }
    Ok(path.to_path_buf())
}

fn validate_pattern_origins(patterns: &[String], origin: &DefinitionOrigin) -> Result<(), String> {
    let Some(boundary_root) = origin.bundle_root.as_ref() else {
        // Legacy local configurations are explicitly operator-selected and
        // retain their existing ability to address external local paths. The
        // future protected local policy will narrow this boundary for apply.
        return Ok(());
    };
    for (index, pattern) in patterns.iter().enumerate() {
        let pattern = pattern.trim();
        let pattern = pattern.strip_prefix('!').unwrap_or(pattern).trim();
        let path = Path::new(pattern);
        if path.is_absolute() {
            return Err(format!(
                "patterns[{}] must be relative to defining config '{}'",
                index,
                origin.defining_config_path.display()
            ));
        }

        // Glob metacharacters prevent canonicalizing the pattern itself. Reject
        // traversal lexically now; execution canonicalizes every concrete match.
        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
        {
            return Err(format!(
                "patterns[{}] escapes source root '{}'",
                index,
                boundary_root.display()
            ));
        }
        if pattern.contains('\\') {
            return Err(format!(
                "patterns[{}] in a fetched definition cannot contain backslashes",
                index
            ));
        }
    }
    Ok(())
}

fn resolve_stdin(data: &str, _mode: ResolverMode) -> Result<ResolvedLoad, String> {
    let mut loaded = decode_with_diagnostics(data, "-")?;
    let cfg = &mut loaded.config;
    // Preserve the legacy stdin contract: the document is typed and receives
    // defaults, but remote entries are not expanded from an origin that has no
    // defining file. Such entries remain in the flat execution plan exactly as
    // they did before origin-aware loading.
    apply_rule_defaults(cfg);

    let working_directory = std::env::current_dir()
        .map_err(|error| format!("resolve stdin working directory: {}", error))?
        .canonicalize()
        .map_err(|error| format!("resolve stdin working directory: {}", error))?;
    let origin = DefinitionOrigin {
        source_identity: SourceIdentity::Stdin {
            working_directory: working_directory.clone(),
        },
        defining_config_path: PathBuf::from("<stdin>"),
        source_relative_config: None,
        base_dir: working_directory,
        bundle_root: None,
        revision: None,
    };

    let definition = ResolvedDefinition {
        cache_path: cfg.cache_path.clone(),
        check_severity: cfg.check_severity,
        fail_severity: cfg.fail_severity,
        preconditions: std::mem::take(&mut cfg.preconditions)
            .into_iter()
            .map(|rule| ResolvedRule {
                rule,
                origin: origin.clone(),
            })
            .collect(),
        rules: std::mem::take(&mut cfg.rules)
            .into_iter()
            .map(|rule| ResolvedRule {
                rule,
                origin: origin.clone(),
            })
            .collect(),
        pattern_groups: vec![ResolvedPatternGroup {
            patterns: std::mem::take(&mut cfg.patterns),
            origin,
        }],
    };

    Ok(ResolvedLoad {
        definition,
        diagnostics: loaded.diagnostics,
        git_dependencies: Vec::new(),
    })
}

fn collect_severity_diagnostics(
    document: &serde_yaml::Value,
    source: &str,
) -> Vec<ConfigDiagnostic> {
    let Some(mapping) = document.as_mapping() else {
        return vec![];
    };

    let mut diagnostics = Vec::new();
    collect_severity_field(
        mapping.get(serde_yaml::Value::String("checkSeverity".to_string())),
        source,
        "checkSeverity",
        &mut diagnostics,
    );
    collect_severity_field(
        mapping.get(serde_yaml::Value::String("failSeverity".to_string())),
        source,
        "failSeverity",
        &mut diagnostics,
    );
    collect_rule_severity_diagnostics(
        mapping.get(serde_yaml::Value::String("preconditions".to_string())),
        source,
        "preconditions",
        &mut diagnostics,
    );
    collect_rule_severity_diagnostics(
        mapping.get(serde_yaml::Value::String("rules".to_string())),
        source,
        "rules",
        &mut diagnostics,
    );

    diagnostics
}

fn collect_rule_severity_diagnostics(
    rules: Option<&serde_yaml::Value>,
    source: &str,
    field: &str,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    let Some(rules) = rules.and_then(serde_yaml::Value::as_sequence) else {
        return;
    };

    for (index, rule) in rules.iter().enumerate() {
        let severity = rule
            .as_mapping()
            .and_then(|mapping| mapping.get(serde_yaml::Value::String("severity".to_string())));
        collect_severity_field(
            severity,
            source,
            &format!("{}[{}].severity", field, index),
            diagnostics,
        );
    }
}

fn collect_severity_field(
    severity: Option<&serde_yaml::Value>,
    source: &str,
    field: &str,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    let Some(value) = severity.and_then(serde_yaml::Value::as_str) else {
        return;
    };
    let canonical = value.to_ascii_lowercase();
    if value == canonical
        || !matches!(
            canonical.as_str(),
            "debug" | "info" | "warn" | "warning" | "error"
        )
    {
        return;
    }

    diagnostics.push(ConfigDiagnostic {
        source: source.to_string(),
        field: field.to_string(),
        value: value.to_string(),
        canonical,
    });
}

/// Parse a remote string to detect git-based resource locators
/// Format: git+<repo>#<ref>:<path>
///   - ref defaults to "main"
///   - path defaults to ".checksy.yaml"
///
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
    use serde::Deserialize;
    use std::process::Command;
    use tempfile::TempDir;

    fn repository_fixture(path: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join(path)
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct StrictConfigCorpus {
        schema_version: u8,
        cases: Vec<StrictConfigCase>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct StrictConfigCase {
        id: String,
        fixture: String,
        expected: String,
        validation_layer: ValidationLayer,
        #[serde(default)]
        error_contains: Option<String>,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "kebab-case")]
    enum ValidationLayer {
        Structural,
        YamlParser,
        RuntimeOnly,
    }

    fn strict_config_corpus() -> StrictConfigCorpus {
        let index_data =
            fs::read_to_string(repository_fixture("strict-config/cases.yaml")).unwrap();
        serde_yaml::from_str(&index_data).unwrap()
    }

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
    fn test_strict_config_contract_fixtures() {
        let root = repository_fixture("strict-config");
        let corpus = strict_config_corpus();
        assert_eq!(corpus.schema_version, 1);

        let mut ids = HashSet::new();
        for case in corpus.cases {
            assert!(
                ids.insert(case.id.clone()),
                "duplicate case id: {}",
                case.id
            );
            let fixture = root.join(&case.fixture);
            assert!(fixture.is_file(), "missing fixture for {}", case.id);

            let fixture_data = fs::read_to_string(&fixture).unwrap();
            let typed_result = serde_yaml::from_str::<Config>(&fixture_data);
            let load_result = load(fixture.to_str().unwrap());
            match case.expected.as_str() {
                "accept" => {
                    assert!(
                        typed_result.is_ok(),
                        "{} should deserialize, got: {:?}",
                        case.id,
                        typed_result.err()
                    );
                    assert!(
                        load_result.is_ok(),
                        "{} should load, got: {:?}",
                        case.id,
                        load_result.err()
                    );
                }
                "reject" => {
                    let typed_error = match typed_result {
                        Ok(_) => panic!("{} should fail typed deserialization", case.id),
                        Err(error) => error.to_string(),
                    };
                    let load_error = match load_result {
                        Ok(_) => panic!("{} should be rejected by load", case.id),
                        Err(error) => error,
                    };
                    if let Some(expected) = case.error_contains {
                        assert!(
                            typed_error.contains(&expected),
                            "{} typed error {:?} did not contain {:?}",
                            case.id,
                            typed_error,
                            expected
                        );
                        assert!(
                            load_error.contains(&expected),
                            "{} load error {:?} did not contain {:?}",
                            case.id,
                            load_error,
                            expected
                        );
                    }
                }
                other => panic!("{} has unknown expectation {:?}", case.id, other),
            }
        }
    }

    #[test]
    fn test_generated_schema_matches_structural_fixture_contract() {
        let root = repository_fixture("strict-config");
        let corpus = strict_config_corpus();
        let generated = crate::schema::configuration_schema();
        let schema = serde_json::to_value(&generated).unwrap();

        jsonschema::draft7::meta::validate(&schema)
            .expect("generated configuration schema must be valid Draft 7");
        let validator =
            jsonschema::draft7::new(&schema).expect("generated configuration schema must compile");
        assert_eq!(
            schema.get("$schema").and_then(serde_json::Value::as_str),
            Some("http://json-schema.org/draft-07/schema#")
        );
        assert_eq!(
            schema,
            serde_json::to_value(crate::schema::configuration_schema()).unwrap(),
            "schema generation must be deterministic"
        );

        let mut structural_count = 0;
        let mut parser_case_ids = HashSet::new();
        let mut runtime_case_ids = HashSet::new();

        for case in corpus.cases {
            let data = fs::read_to_string(root.join(&case.fixture)).unwrap();
            let typed_accepts = serde_yaml::from_str::<Config>(&data).is_ok();

            match case.validation_layer {
                ValidationLayer::Structural => {
                    structural_count += 1;
                    let yaml: serde_yaml::Value =
                        serde_yaml::from_str(&data).unwrap_or_else(|error| {
                            panic!("{} must parse as YAML: {}", case.id, error)
                        });
                    let instance = serde_json::to_value(yaml).unwrap();
                    let schema_accepts = validator.is_valid(&instance);
                    let expected_accepts = case.expected == "accept";
                    assert_eq!(
                        schema_accepts, expected_accepts,
                        "{} schema result did not match the fixture expectation",
                        case.id
                    );
                    assert_eq!(
                        schema_accepts, typed_accepts,
                        "{} differed between schema and typed deserialization",
                        case.id
                    );
                }
                ValidationLayer::YamlParser => {
                    assert_eq!(case.expected, "reject", "{} must be rejected", case.id);
                    assert!(
                        serde_yaml::from_str::<serde_yaml::Value>(&data).is_err(),
                        "{} must be rejected by the YAML parser",
                        case.id
                    );
                    assert!(!typed_accepts, "{} must fail typed parsing", case.id);
                    parser_case_ids.insert(case.id);
                }
                ValidationLayer::RuntimeOnly => {
                    assert_eq!(case.expected, "reject", "{} must be rejected", case.id);
                    let yaml: serde_yaml::Value =
                        serde_yaml::from_str(&data).unwrap_or_else(|error| {
                            panic!("{} must parse as YAML: {}", case.id, error)
                        });
                    let instance = serde_json::to_value(yaml).unwrap();
                    assert!(
                        validator.is_valid(&instance),
                        "{} must pass the structural schema",
                        case.id
                    );
                    assert!(!typed_accepts, "{} must fail runtime validation", case.id);
                    runtime_case_ids.insert(case.id);
                }
            }
        }

        assert_eq!(structural_count, 24);
        assert_eq!(
            parser_case_ids,
            HashSet::from([
                "duplicate-top-level-key".to_string(),
                "duplicate-rule-key".to_string(),
            ])
        );
        assert_eq!(
            runtime_case_ids,
            HashSet::from(["invalid-pattern".to_string()])
        );
    }

    #[test]
    fn test_severity_diagnostics_are_location_aware_and_lowercase_aliases_are_quiet() {
        let loaded = decode_with_diagnostics(
            "checkSeverity: ERROR\nfailSeverity: warning\npreconditions:\n  - check: ok\n    severity: INFO\nrules:\n  - check: ok\n    severity: WaRn\n",
            "-",
        )
        .unwrap();

        let messages: Vec<String> = loaded.diagnostics.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "warning: <stdin>: checkSeverity: non-lowercase severity 'ERROR' is deprecated; use 'error'",
                "warning: <stdin>: preconditions[0].severity: non-lowercase severity 'INFO' is deprecated; use 'info'",
                "warning: <stdin>: rules[0].severity: non-lowercase severity 'WaRn' is deprecated; use 'warn'",
            ]
        );
    }

    #[test]
    fn test_nested_severity_diagnostics_follow_successful_load_order() {
        let dir = TempDir::new().unwrap();
        let nested_path = dir.path().join("nested.yaml");
        fs::write(
            &nested_path,
            "checkSeverity: INFO\nrules:\n  - check: echo nested\n    severity: WARNING\n",
        )
        .unwrap();
        let root_path = dir.path().join("root.yaml");
        fs::write(
            &root_path,
            "failSeverity: ERROR\nrules:\n  - remote: nested.yaml\n",
        )
        .unwrap();

        let loaded = load_with_diagnostics(root_path.to_str().unwrap()).unwrap();
        let messages: Vec<String> = loaded.diagnostics.iter().map(ToString::to_string).collect();
        assert_eq!(messages.len(), 3);
        assert!(messages[0].contains("root.yaml: failSeverity"));
        assert!(messages[1].contains("nested.yaml: checkSeverity"));
        assert!(messages[2].contains("nested.yaml: rules[0].severity"));

        assert!(load(root_path.to_str().unwrap()).is_ok());
    }

    #[test]
    fn test_existing_valid_config_fixtures_remain_loadable() {
        let fixtures = [
            "check-logs/.checksy.yaml",
            "default-severity/.checksy.yaml",
            "fix-behavior/.checksy.yaml",
            "happy-path/.checksy.yaml",
            "hint-test/.checksy.yaml",
            "inline-check/.checksy.yaml",
            "preconditions/.checksy.yaml",
            "remote-config/.checksy.yaml",
            "remote-config/inherit-parent.yaml",
            "remote-config/nested/top.yaml",
            "remote-config/with-preconditions.yaml",
            "rule-files/.checksy.yaml",
        ];

        for fixture in fixtures {
            let path = repository_fixture(fixture);
            let result = load(path.to_str().unwrap());
            assert!(
                result.is_ok(),
                "existing fixture {} should remain valid, got: {:?}",
                fixture,
                result.err()
            );
        }
    }

    #[test]
    fn test_stdin_uses_current_directory_without_remote_expansion() {
        let loaded = resolve_stdin(
            concat!(
                "rules:\n",
                "  - check: 'true'\n",
                "  - remote: child.yaml\n",
                "patterns:\n",
                "  - '*.sh'\n"
            ),
            ResolverMode::CachedOnly,
        )
        .unwrap();
        let current = std::env::current_dir().unwrap().canonicalize().unwrap();

        assert_eq!(loaded.definition.rules.len(), 2);
        assert_eq!(loaded.definition.rules[0].origin.base_dir, current);
        assert_eq!(
            loaded.definition.rules[1].rule.remote.as_deref(),
            Some("child.yaml")
        );
        assert_eq!(loaded.definition.pattern_groups[0].origin.base_dir, current);
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
    fn test_circular_reference_is_rejected() {
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
        let error = result.expect_err("circular definitions must fail closed");
        assert!(error.contains("circular remote definition reference"));
        let a = path_a.canonicalize().unwrap().display().to_string();
        let b = path_b.canonicalize().unwrap().display().to_string();
        assert!(error.contains(&format!("{} -> {} -> {}", a, b, a)));
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

    #[test]
    fn test_resolved_definition_keeps_origins_defaults_and_pattern_groups() {
        let dir = TempDir::new().unwrap();
        let nested_dir = dir.path().join("nested");
        fs::create_dir(&nested_dir).unwrap();
        let child_path = nested_dir.join("child.yaml");
        fs::write(
            &child_path,
            "rules:\n  - name: child\n    check: test -f child.txt\npatterns:\n  - '*.sh'\n  - '!skip.sh'\n",
        )
        .unwrap();
        let root_path = dir.path().join("root.yaml");
        fs::write(
            &root_path,
            "checkSeverity: warn\nrules:\n  - remote: nested/child.yaml\n  - name: root\n    check: 'true'\npatterns:\n  - 'root/*.sh'\n",
        )
        .unwrap();

        let loaded = load_resolved_with_diagnostics(root_path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.definition.rules.len(), 2);
        assert_eq!(
            loaded.definition.rules[0].rule.name.as_deref(),
            Some("child")
        );
        assert_eq!(
            loaded.definition.rules[0].rule.severity,
            Some(Severity::Warning)
        );
        assert_eq!(
            loaded.definition.rules[0].origin.base_dir,
            nested_dir.canonicalize().unwrap()
        );
        assert_eq!(
            loaded.definition.rules[1].origin.base_dir,
            dir.path().canonicalize().unwrap()
        );
        assert_eq!(loaded.definition.pattern_groups.len(), 2);
        assert_eq!(loaded.definition.pattern_groups[0].patterns, ["root/*.sh"]);
        assert_eq!(
            loaded.definition.pattern_groups[1].patterns,
            ["*.sh", "!skip.sh"]
        );
        assert_eq!(
            loaded.definition.rules[0].origin.source_identity,
            loaded.definition.rules[1].origin.source_identity
        );
    }

    #[test]
    fn test_nested_sections_preserve_order_and_inherited_defaults() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pre-child.yaml"),
            concat!(
                "preconditions:\n",
                "  - name: pre-child-precondition\n",
                "    check: 'true'\n",
                "rules:\n",
                "  - name: pre-child-rule\n",
                "    check: 'true'\n",
                "  - name: pre-child-explicit\n",
                "    check: 'true'\n",
                "    severity: debug\n"
            ),
        )
        .unwrap();
        fs::write(
            dir.path().join("rule-child.yaml"),
            concat!(
                "preconditions:\n",
                "  - name: rule-child-precondition\n",
                "    check: 'true'\n",
                "rules:\n",
                "  - name: rule-child-rule\n",
                "    check: 'true'\n"
            ),
        )
        .unwrap();
        let root_path = dir.path().join("root.yaml");
        fs::write(
            &root_path,
            concat!(
                "checkSeverity: warn\n",
                "preconditions:\n",
                "  - name: root-pre-before\n",
                "    check: 'true'\n",
                "  - remote: pre-child.yaml\n",
                "  - name: root-pre-after\n",
                "    check: 'true'\n",
                "rules:\n",
                "  - name: root-rule-before\n",
                "    check: 'true'\n",
                "  - remote: rule-child.yaml\n",
                "  - name: root-rule-after\n",
                "    check: 'true'\n"
            ),
        )
        .unwrap();

        let loaded = load_resolved_with_diagnostics(root_path.to_str().unwrap()).unwrap();
        let precondition_names: Vec<&str> = loaded
            .definition
            .preconditions
            .iter()
            .map(|resolved| resolved.rule.name.as_deref().unwrap())
            .collect();
        assert_eq!(
            precondition_names,
            [
                "root-pre-before",
                "pre-child-precondition",
                "pre-child-rule",
                "pre-child-explicit",
                "root-pre-after"
            ]
        );
        let rule_names: Vec<&str> = loaded
            .definition
            .rules
            .iter()
            .map(|resolved| resolved.rule.name.as_deref().unwrap())
            .collect();
        assert_eq!(
            rule_names,
            [
                "root-rule-before",
                "rule-child-precondition",
                "rule-child-rule",
                "root-rule-after"
            ]
        );
        assert!(loaded
            .definition
            .preconditions
            .iter()
            .filter(|resolved| resolved.rule.name.as_deref() != Some("pre-child-explicit"))
            .all(|resolved| resolved.rule.severity == Some(Severity::Warning)));
        assert_eq!(
            loaded.definition.preconditions[3].rule.severity,
            Some(Severity::Debug)
        );
        assert!(loaded
            .definition
            .rules
            .iter()
            .all(|resolved| resolved.rule.severity == Some(Severity::Warning)));
    }

    #[test]
    fn test_completed_definition_is_deduplicated_but_active_cycle_errors() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("shared.yaml"),
            "rules:\n  - name: shared\n    check: 'true'\n",
        )
        .unwrap();
        let root_path = dir.path().join("root.yaml");
        fs::write(
            &root_path,
            "rules:\n  - remote: shared.yaml\n  - remote: shared.yaml\n",
        )
        .unwrap();

        let loaded = load_resolved_with_diagnostics(root_path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.definition.rules.len(), 1);
        assert_eq!(
            loaded.definition.rules[0].rule.name.as_deref(),
            Some("shared")
        );

        fs::write(
            dir.path().join("shared.yaml"),
            "rules:\n  - remote: root.yaml\n",
        )
        .unwrap();
        let error = load_resolved_with_diagnostics(root_path.to_str().unwrap()).unwrap_err();
        assert!(error.contains("circular remote definition reference"));
    }

    #[test]
    fn test_legacy_local_remote_can_resolve_outside_selected_root() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(
            dir.path().join("outside.yaml"),
            "rules:\n  - check: 'true'\n",
        )
        .unwrap();
        let root_path = source.join("root.yaml");
        fs::write(&root_path, "rules:\n  - remote: ../outside.yaml\n").unwrap();

        let loaded = load_resolved_with_diagnostics(root_path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.definition.rules.len(), 1);
        assert_eq!(loaded.definition.rules[0].origin.base_dir, dir.path());
        assert_eq!(
            loaded.definition.rules[0].origin.source_relative_config,
            None
        );
    }

    #[test]
    fn test_collect_dependencies_reports_every_missing_git_remote_in_order() {
        let dir = TempDir::new().unwrap();
        let root_path = dir.path().join("root.yaml");
        fs::write(
            &root_path,
            concat!(
                "cachePath: cache\n",
                "rules:\n",
                "  - remote: git+https://example.invalid/one.git#main:checks.yaml\n",
                "  - remote: git+https://example.invalid/two.git#stable:checks.yaml\n",
                "  - check: 'true'\n"
            ),
        )
        .unwrap();

        let loaded =
            load_resolved_with_mode(root_path.to_str().unwrap(), ResolverMode::CacheMissing)
                .unwrap();
        assert_eq!(loaded.git_dependencies.len(), 2);
        assert_eq!(
            loaded.git_dependencies[0].remote.repo,
            "https://example.invalid/one.git"
        );
        assert_eq!(loaded.git_dependencies[1].remote.ref_, "stable");
        assert!(loaded.git_dependencies.iter().all(|item| !item.cached));
        assert_eq!(loaded.definition.rules.len(), 1);
        assert!(loaded
            .git_dependencies
            .iter()
            .all(|item| item.cache_root == dir.path().join("cache")));
    }

    #[test]
    fn test_install_frontier_defers_unrefreshed_cached_git_parsing() {
        let dir = TempDir::new().unwrap();
        let root_path = dir.path().join("root.yaml");
        let repo = "https://example.invalid/stale.git";
        fs::write(
            &root_path,
            format!(
                "cachePath: cache\nrules:\n  - remote: git+{}#main:checks.yaml\n",
                repo
            ),
        )
        .unwrap();
        let cache = CacheManager::new(dir.path(), Some("cache"));
        initialize_git_checkout(
            &cache.ref_cache_path(repo, "main"),
            // A stale cache may be malformed even when the upstream ref has
            // since been repaired. Install must refresh this frontier before
            // attempting to decode it.
            "rules:\n  - check: true\n",
            None,
        );

        let frontier = load_resolved_for_install(root_path.to_str().unwrap(), &HashSet::new())
            .expect("unrefreshed cached Git should be reported without being decoded");
        assert_eq!(frontier.git_dependencies.len(), 1);
        assert!(frontier.git_dependencies[0].cached);

        let expanded = HashSet::from([(repo.to_string(), "main".to_string())]);
        let error = load_resolved_for_install(root_path.to_str().unwrap(), &expanded).unwrap_err();
        assert!(
            error.contains("invalid type: expected a YAML string"),
            "{error}"
        );
    }

    #[test]
    fn test_install_expands_every_config_in_a_refreshed_checkout() {
        let dir = TempDir::new().unwrap();
        let root_path = dir.path().join("root.yaml");
        let first_repo = "https://example.invalid/shared.git";
        let nested_repo = "https://example.invalid/nested.git";
        fs::write(
            &root_path,
            format!(
                concat!(
                    "cachePath: cache\n",
                    "rules:\n",
                    "  - remote: git+{}#main:checks.yaml\n",
                    "  - remote: git+{}#main:other.yaml\n"
                ),
                first_repo, first_repo
            ),
        )
        .unwrap();
        let cache = CacheManager::new(dir.path(), Some("cache"));
        initialize_git_checkout(
            &cache.ref_cache_path(first_repo, "main"),
            "rules:\n  - check: 'true'\n",
            Some((
                "other.yaml",
                &format!("rules:\n  - remote: git+{}#main:checks.yaml\n", nested_repo),
            )),
        );

        let expanded = HashSet::from([(first_repo.to_string(), "main".to_string())]);
        let discovered = load_resolved_for_install(root_path.to_str().unwrap(), &expanded).unwrap();

        assert_eq!(discovered.git_dependencies.len(), 2);
        assert_eq!(discovered.git_dependencies[0].remote.repo, first_repo);
        assert_eq!(discovered.git_dependencies[1].remote.repo, nested_repo);
    }

    #[test]
    fn test_same_git_definition_is_deduplicated_across_entry_paths() {
        let dir = TempDir::new().unwrap();
        let root_path = dir.path().join("root.yaml");
        let repo = "https://example.invalid/shared.git";
        fs::write(
            &root_path,
            format!(
                concat!(
                    "cachePath: cache\n",
                    "rules:\n",
                    "  - remote: git+{}#main:checks.yaml\n",
                    "  - remote: git+{}#main:other.yaml\n"
                ),
                repo, repo
            ),
        )
        .unwrap();
        let cache = CacheManager::new(dir.path(), Some("cache"));
        initialize_git_checkout(
            &cache.ref_cache_path(repo, "main"),
            "rules:\n  - remote: other.yaml\n",
            Some(("other.yaml", "rules:\n  - name: once\n    check: 'true'\n")),
        );

        let loaded = load_resolved_with_diagnostics(root_path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.definition.rules.len(), 1);
        assert_eq!(
            loaded.definition.rules[0].rule.name.as_deref(),
            Some("once")
        );
    }

    #[test]
    fn test_nested_git_uses_root_cache_anchor_and_preserves_bundle_origin() {
        let dir = TempDir::new().unwrap();
        let root_path = dir.path().join("root.yaml");
        let first_repo = "https://example.invalid/first.git";
        let second_repo = "https://example.invalid/second.git";
        fs::write(
            &root_path,
            format!(
                "cachePath: cache\nrules:\n  - remote: git+{}#main:checks.yaml\n",
                first_repo
            ),
        )
        .unwrap();
        let cache = CacheManager::new(dir.path(), Some("cache"));
        let first_checkout = cache.ref_cache_path(first_repo, "main");
        initialize_git_checkout(
            &first_checkout,
            &format!(
                "cachePath: nested-cache-must-be-ignored\nrules:\n  - remote: child.yaml\n  - remote: git+{}#main:checks.yaml\n",
                second_repo
            ),
            Some(("child.yaml", "rules:\n  - name: first-child\n    check: 'true'\n")),
        );
        let second_checkout = cache.ref_cache_path(second_repo, "main");
        initialize_git_checkout(
            &second_checkout,
            "rules:\n  - name: second-child\n    check: 'true'\n",
            None,
        );

        let loaded =
            load_resolved_with_mode(root_path.to_str().unwrap(), ResolverMode::CacheMissing)
                .unwrap();
        assert_eq!(loaded.git_dependencies.len(), 2);
        assert!(loaded.git_dependencies.iter().all(|item| item.cached));
        assert!(loaded
            .git_dependencies
            .iter()
            .all(|item| item.cache_root == dir.path().join("cache")));
        assert_eq!(loaded.definition.rules.len(), 2);
        assert_eq!(
            loaded.definition.rules[0].origin.bundle_root,
            Some(first_checkout.canonicalize().unwrap())
        );
        assert_eq!(
            loaded.definition.rules[1].origin.bundle_root,
            Some(second_checkout.canonicalize().unwrap())
        );
        assert!(matches!(
            loaded.definition.rules[0].origin.source_identity,
            SourceIdentity::Git { .. }
        ));
    }

    #[test]
    fn test_nested_file_parent_traversal_can_remain_inside_git_checkout() {
        let dir = TempDir::new().unwrap();
        let root_path = dir.path().join("root.yaml");
        let repo = "https://example.invalid/inside.git";
        fs::write(
            &root_path,
            format!(
                "cachePath: cache\nrules:\n  - remote: git+{}#main:nested/entry.yaml\n",
                repo
            ),
        )
        .unwrap();
        let cache = CacheManager::new(dir.path(), Some("cache"));
        let checkout = cache.ref_cache_path(repo, "main");
        initialize_git_checkout(
            &checkout,
            "rules:\n  - name: shared\n    check: 'true'\n",
            Some(("nested/entry.yaml", "rules:\n  - remote: ../checks.yaml\n")),
        );

        let loaded = load_resolved_with_diagnostics(root_path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.definition.rules.len(), 1);
        assert_eq!(
            loaded.definition.rules[0].rule.name.as_deref(),
            Some("shared")
        );
        assert_eq!(
            loaded.definition.rules[0].origin.bundle_root,
            Some(checkout.canonicalize().unwrap())
        );
    }

    #[test]
    fn test_nested_absolute_file_path_in_git_is_rejected_before_lookup() {
        let dir = TempDir::new().unwrap();
        let root_path = dir.path().join("root.yaml");
        let repo = "https://example.invalid/absolute-nested.git";
        fs::write(
            &root_path,
            format!(
                "cachePath: cache\nrules:\n  - remote: git+{}#main:checks.yaml\n",
                repo
            ),
        )
        .unwrap();
        let cache = CacheManager::new(dir.path(), Some("cache"));
        initialize_git_checkout(
            &cache.ref_cache_path(repo, "main"),
            "rules:\n  - remote: /definitely/not/a/checksy/config.yaml\n",
            None,
        );

        let error = load_resolved_with_diagnostics(root_path.to_str().unwrap()).unwrap_err();
        assert!(error.contains("must be relative"), "{error}");
    }

    #[test]
    fn test_fetched_config_cannot_escape_git_bundle() {
        let dir = TempDir::new().unwrap();
        let root_path = dir.path().join("root.yaml");
        let repo = "https://example.invalid/confined.git";
        fs::write(
            &root_path,
            format!(
                "cachePath: cache\nrules:\n  - remote: git+{}#main:../outside.yaml\n",
                repo
            ),
        )
        .unwrap();
        let error =
            load_resolved_with_mode(root_path.to_str().unwrap(), ResolverMode::CacheMissing)
                .unwrap_err();
        assert!(error.contains("canonical relative path"), "{}", error);
    }

    #[test]
    fn test_fetched_config_paths_are_normalized_repository_relative_paths() {
        for invalid in [
            "/checks.yaml",
            "../checks.yaml",
            "nested/../checks.yaml",
            "nested\\checks.yaml",
            "nested//checks.yaml",
        ] {
            let error = validate_fetched_relative_path(invalid).unwrap_err();
            assert!(error.contains("git config path"), "{invalid}: {error}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_fetched_config_symlink_escape_is_rejected() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new().unwrap();
        let root_path = dir.path().join("root.yaml");
        let repo = "https://example.invalid/symlink.git";
        fs::write(
            &root_path,
            format!(
                "cachePath: cache\nrules:\n  - remote: git+{}#main:escape.yaml\n",
                repo
            ),
        )
        .unwrap();
        let cache = CacheManager::new(dir.path(), Some("cache"));
        let checkout = cache.ref_cache_path(repo, "main");
        initialize_git_checkout(&checkout, "rules: []\n", None);
        let outside = dir.path().join("outside.yaml");
        fs::write(&outside, "rules: []\n").unwrap();
        symlink(&outside, checkout.join("escape.yaml")).unwrap();

        let error = load_resolved_with_diagnostics(root_path.to_str().unwrap()).unwrap_err();
        assert!(error.contains("escapes source root"), "{}", error);
    }

    #[cfg(unix)]
    #[test]
    fn test_fetched_config_symlink_with_internal_target_is_allowed() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new().unwrap();
        let root_path = dir.path().join("root.yaml");
        let repo = "https://example.invalid/internal-symlink.git";
        fs::write(
            &root_path,
            format!(
                "cachePath: cache\nrules:\n  - remote: git+{}#main:alias.yaml\n",
                repo
            ),
        )
        .unwrap();
        let cache = CacheManager::new(dir.path(), Some("cache"));
        let checkout = cache.ref_cache_path(repo, "main");
        initialize_git_checkout(
            &checkout,
            "rules:\n  - name: internal\n    check: 'true'\n",
            None,
        );
        symlink("checks.yaml", checkout.join("alias.yaml")).unwrap();

        let loaded = load_resolved_with_diagnostics(root_path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.definition.rules.len(), 1);
        assert_eq!(
            loaded.definition.rules[0].rule.name.as_deref(),
            Some("internal")
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_fetched_checkout_slot_cannot_be_a_symlink() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new().unwrap();
        let root_path = dir.path().join("root.yaml");
        let repo = "https://example.invalid/symlink-root.git";
        fs::write(
            &root_path,
            format!(
                "cachePath: cache\nrules:\n  - remote: git+{}#main:checks.yaml\n",
                repo
            ),
        )
        .unwrap();
        let cache = CacheManager::new(dir.path(), Some("cache"));
        let checkout_slot = cache.ref_cache_path(repo, "main");
        let outside_checkout = dir.path().join("outside-checkout");
        initialize_git_checkout(&outside_checkout, "rules: []\n", None);
        fs::create_dir_all(checkout_slot.parent().unwrap()).unwrap();
        symlink(&outside_checkout, &checkout_slot).unwrap();

        let error = load_resolved_with_diagnostics(root_path.to_str().unwrap()).unwrap_err();
        assert!(error.contains("symbolic link"), "{error}");
    }

    fn initialize_git_checkout(
        checkout: &Path,
        root_config: &str,
        extra_file: Option<(&str, &str)>,
    ) {
        fs::create_dir_all(checkout).unwrap();
        assert!(Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(checkout)
            .status()
            .unwrap()
            .success());
        fs::write(checkout.join("checks.yaml"), root_config).unwrap();
        if let Some((path, contents)) = extra_file {
            let path = checkout.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, contents).unwrap();
        }
        assert!(Command::new("git")
            .arg("-C")
            .arg(checkout)
            .args(["add", "."])
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .arg("-C")
            .arg(checkout)
            .args([
                "-c",
                "user.name=Checksy Test",
                "-c",
                "user.email=checksy@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "fixture",
            ])
            .status()
            .unwrap()
            .success());
    }
}
