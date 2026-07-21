use crate::resolved::{DefinitionOrigin, ResolvedDefinition, ResolvedPatternGroup, ResolvedRule};
use crate::schema::{severity_order, Config, Rule, Severity};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const DEFAULT_RULE_SEVERITY: Severity = Severity::Error;

#[derive(Debug, Clone)]
pub struct Options {
    pub config: Config,
    pub workdir: String,
    pub min_severity: Severity,
    pub fail_severity: Severity,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedOptions {
    pub(crate) definition: ResolvedDefinition,
    pub(crate) min_severity: Severity,
    pub(crate) fail_severity: Severity,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedRuleFile {
    origin: DefinitionOrigin,
    display_path: String,
    executable_path: PathBuf,
}

#[derive(Debug)]
pub struct Report {
    pub rules: Vec<RuleResult>,
    pub fail_severity: Severity,
}

impl Clone for Report {
    fn clone(&self) -> Self {
        Report {
            rules: self.rules.clone(),
            fail_severity: self.fail_severity,
        }
    }
}

#[derive(Debug)]
pub struct RuleResult {
    pub rule: Rule,
    pub err: Option<Box<dyn std::error::Error>>,
    pub stdout: String,
    pub stderr: String,
}

impl Clone for RuleResult {
    fn clone(&self) -> Self {
        RuleResult {
            rule: self.rule.clone(),
            err: None,
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
        }
    }
}

impl RuleResult {
    pub fn success(&self) -> bool {
        self.err.is_none()
    }

    pub fn name(&self) -> String {
        self.rule
            .name
            .clone()
            .unwrap_or_else(|| self.rule.check.clone().unwrap_or_default())
    }

    pub fn severity(&self) -> Severity {
        normalize_rule_severity(self.rule.severity.clone().unwrap_or(DEFAULT_RULE_SEVERITY))
    }

    pub fn should_fail(&self, threshold: Severity) -> bool {
        if self.success() {
            return false;
        }
        let normalized = normalize_fail_severity(threshold);
        severity_order(self.severity()) >= severity_order(normalized)
    }
}

impl Report {
    pub fn has_failures(&self) -> bool {
        let threshold = normalize_fail_severity(self.fail_severity);
        self.rules.iter().any(|r| r.should_fail(threshold))
    }

    pub fn failures(&self) -> Vec<RuleResult> {
        let threshold = normalize_fail_severity(self.fail_severity);
        self.rules
            .iter()
            .filter(|r| r.should_fail(threshold))
            .map(|r| (*r).clone())
            .collect()
    }
}

pub fn diagnose(opts: Options) -> Result<Report, String> {
    let workdir = if opts.workdir.is_empty() {
        "."
    } else {
        &opts.workdir
    };
    let mut results = run_preconditions(&opts, workdir);

    let rules = filter_rules(&opts.config, opts.min_severity);
    for rule in rules {
        results.push(run_rule(rule, workdir));
    }

    // Keep the public flat API's legacy timing: rules may create files that its
    // pattern phase subsequently discovers. The resolved CLI path preflights
    // fetched patterns before any command instead.
    let file_paths = expand_rule_files(workdir, &opts.config.patterns)?;
    for rel_path in file_paths {
        results.push(run_rule_file(workdir, &rel_path));
    }

    Ok(Report {
        rules: results,
        fail_severity: opts.fail_severity,
    })
}

pub(crate) fn diagnose_resolved(opts: ResolvedOptions) -> Result<Report, String> {
    // Pattern expansion is part of execution-plan validation. Finish it before
    // invoking arbitrary commands so a confined-path failure cannot arrive
    // after a precondition or rule has already changed the host.
    let rule_files = expand_resolved_rule_files(&opts.definition.pattern_groups)?;
    let mut results = Vec::new();

    for rule in filter_resolved_rules(&opts.definition.preconditions, opts.min_severity) {
        results.push(run_resolved_rule(rule));
    }

    for rule in filter_resolved_rules(&opts.definition.rules, opts.min_severity) {
        results.push(run_resolved_rule(rule));
    }

    for rule_file in rule_files {
        results.push(run_resolved_rule_file(rule_file));
    }

    Ok(Report {
        rules: results,
        fail_severity: opts.fail_severity,
    })
}

fn run_preconditions(opts: &Options, workdir: &str) -> Vec<RuleResult> {
    let preconditions = filter_preconditions(&opts.config, opts.min_severity);
    preconditions
        .into_iter()
        .map(|r| run_rule(r, workdir))
        .collect()
}

pub fn filter_rules(cfg: &Config, min: Severity) -> Vec<Rule> {
    let min_severity = normalize_min_severity(min);
    cfg.rules
        .iter()
        .filter(|r| rule_meets_severity(r, min_severity))
        .cloned()
        .collect()
}

pub fn filter_preconditions(cfg: &Config, min: Severity) -> Vec<Rule> {
    let min_severity = normalize_min_severity(min);
    cfg.preconditions
        .iter()
        .filter(|r| rule_meets_severity(r, min_severity))
        .cloned()
        .collect()
}

pub(crate) fn filter_resolved_rules(rules: &[ResolvedRule], min: Severity) -> Vec<ResolvedRule> {
    let min_severity = normalize_min_severity(min);
    rules
        .iter()
        .filter(|resolved| rule_meets_severity(&resolved.rule, min_severity))
        .cloned()
        .collect()
}

pub fn run_rule(rule: Rule, workdir: &str) -> RuleResult {
    run_rule_in_dir(rule, Path::new(workdir))
}

pub(crate) fn run_resolved_rule(rule: ResolvedRule) -> RuleResult {
    run_rule_in_dir(rule.rule, &rule.origin.base_dir)
}

fn run_rule_in_dir(rule: Rule, workdir: &Path) -> RuleResult {
    let script = rule.check.clone().unwrap_or_else(|| "true".to_string());
    let script = if script.ends_with('\n') {
        script
    } else {
        format!("{}\n", script)
    };

    let output = Command::new("bash")
        .current_dir(workdir)
        .arg("-c")
        .arg(&script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(out) => {
            let exit_error = if !out.status.success() {
                Some(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("command failed with exit code: {:?}", out.status.code()),
                )) as Box<dyn std::error::Error>)
            } else {
                None
            };
            RuleResult {
                rule,
                err: exit_error,
                stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            }
        }
        Err(e) => RuleResult {
            rule,
            err: Some(Box::new(e) as Box<dyn std::error::Error>),
            stdout: String::new(),
            stderr: String::new(),
        },
    }
}

pub(crate) fn expand_resolved_rule_files(
    groups: &[ResolvedPatternGroup],
) -> Result<Vec<ResolvedRuleFile>, String> {
    let mut files = Vec::new();
    for group in groups {
        files.extend(expand_resolved_pattern_group(group)?);
    }
    Ok(files)
}

fn expand_resolved_pattern_group(
    group: &ResolvedPatternGroup,
) -> Result<Vec<ResolvedRuleFile>, String> {
    validate_origin_confinement(&group.origin)?;

    let mut positive = Vec::new();
    let mut negative = Vec::new();
    for pattern in &group.patterns {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            continue;
        }

        let (is_negative, pattern) = match pattern.strip_prefix('!') {
            Some(pattern) => (true, pattern.trim()),
            None => (false, pattern),
        };
        validate_confined_pattern(pattern, &group.origin)?;
        if is_negative {
            negative.push(pattern.to_string());
        } else {
            positive.push(pattern.to_string());
        }
    }

    if positive.is_empty() && group.origin.bundle_root.is_none() {
        // Preserve trusted local legacy behavior. Fetched groups still expand
        // negative-only patterns so a referenced symlink escape is rejected
        // during preflight even though the group would select no scripts.
        return Ok(Vec::new());
    }

    let mut included = BTreeMap::<String, PathBuf>::new();
    for pattern in positive {
        for matched in matched_paths(&group.origin, &pattern)? {
            if matched.executable_path.is_file() {
                included.insert(matched.display_path, matched.executable_path);
            }
        }
    }

    for pattern in negative {
        for matched in matched_paths(&group.origin, &pattern)? {
            included.remove(&matched.display_path);
        }
    }

    Ok(included
        .into_iter()
        .map(|(display_path, executable_path)| ResolvedRuleFile {
            origin: group.origin.clone(),
            display_path,
            executable_path,
        })
        .collect())
}

struct MatchedPath {
    display_path: String,
    executable_path: PathBuf,
}

fn matched_paths(origin: &DefinitionOrigin, pattern: &str) -> Result<Vec<MatchedPath>, String> {
    if let Some(bundle_root) = &origin.bundle_root {
        return matched_confined_paths(origin, pattern, bundle_root);
    }

    let glob_path = origin.base_dir.join(pattern);
    let entries = glob::glob(glob_path.to_string_lossy().as_ref()).map_err(|error| {
        format!(
            "invalid pattern '{}' in '{}': {}",
            pattern,
            origin.defining_config_path.display(),
            error
        )
    })?;
    let mut matched = Vec::new();

    for entry in entries {
        let Ok(entry) = entry else {
            // The legacy local glob path ignored per-entry traversal errors.
            // Fetched groups use the confined walker below and fail closed.
            continue;
        };
        let relative = match entry.strip_prefix(&origin.base_dir) {
            Ok(relative) => relative,
            Err(_) => {
                // Legacy local absolute patterns have always ignored matches
                // outside the configured workdir.
                continue;
            }
        };
        let display_path = relative.to_string_lossy().replace('\\', "/");

        matched.push(MatchedPath {
            display_path,
            executable_path: entry,
        });
    }

    Ok(matched)
}

/// Expand a fetched pattern component by component without asking `glob` to
/// traverse directories first. Every matching directory (including wildcard
/// and `**` matches) is canonicalized and confined before it can be read, so a
/// symlink to an external directory is rejected even when it contains no final
/// file match.
fn matched_confined_paths(
    origin: &DefinitionOrigin,
    pattern: &str,
    bundle_root: &Path,
) -> Result<Vec<MatchedPath>, String> {
    let require_directory = pattern.ends_with('/');
    let components: Vec<&str> = pattern.split_terminator('/').collect();
    if components.is_empty() {
        return Ok(Vec::new());
    }

    let mut matched = Vec::new();
    let mut active = HashSet::new();
    walk_confined_pattern(
        origin,
        bundle_root,
        &origin.base_dir,
        Path::new(""),
        &components,
        0,
        require_directory,
        &mut active,
        &mut matched,
    )?;
    Ok(matched)
}

#[allow(clippy::too_many_arguments)]
fn walk_confined_pattern(
    origin: &DefinitionOrigin,
    bundle_root: &Path,
    directory: &Path,
    display_directory: &Path,
    components: &[&str],
    index: usize,
    require_directory: bool,
    active: &mut HashSet<(PathBuf, usize)>,
    matched: &mut Vec<MatchedPath>,
) -> Result<(), String> {
    if index >= components.len() {
        return Ok(());
    }

    let canonical_directory = directory.canonicalize().map_err(|error| {
        format!(
            "failed to resolve pattern directory '{}' from '{}': {}",
            directory.display(),
            origin.defining_config_path.display(),
            error
        )
    })?;
    ensure_fetched_path(
        origin,
        bundle_root,
        &canonical_directory,
        directory,
        components[index],
    )?;
    let active_key = (canonical_directory.clone(), index);
    if !active.insert(active_key.clone()) {
        return Ok(());
    }

    let result = walk_confined_pattern_inner(
        origin,
        bundle_root,
        &canonical_directory,
        display_directory,
        components,
        index,
        require_directory,
        active,
        matched,
    );
    active.remove(&active_key);
    result
}

#[allow(clippy::too_many_arguments)]
fn walk_confined_pattern_inner(
    origin: &DefinitionOrigin,
    bundle_root: &Path,
    directory: &Path,
    display_directory: &Path,
    components: &[&str],
    index: usize,
    require_directory: bool,
    active: &mut HashSet<(PathBuf, usize)>,
    matched: &mut Vec<MatchedPath>,
) -> Result<(), String> {
    let component = components[index];
    if component == "**" {
        // Match glob 0.3's recursive-component iterator exactly. Ordinarily
        // `**` can consume zero components. Its iterator does not make that
        // transition when the immediately following component is empty or
        // `.`, however, so those accepted patterns match nothing. We still
        // traverse the recursive prefix to preflight fetched symlink escapes.
        let next_is_nonmatching_special = components
            .get(index + 1)
            .is_some_and(|next| next.is_empty() || *next == ".");
        if !next_is_nonmatching_special {
            walk_confined_pattern(
                origin,
                bundle_root,
                directory,
                display_directory,
                components,
                index + 1,
                require_directory,
                active,
                matched,
            )?;
        }

        for entry in sorted_directory_entries(directory, origin, component)? {
            let entry_path = entry.path();
            let display_path = display_directory.join(entry.file_name());
            let canonical = canonicalize_pattern_entry(origin, &entry_path)?;
            ensure_fetched_path(origin, bundle_root, &canonical, &entry_path, component)?;

            if canonical.is_dir() {
                walk_confined_pattern(
                    origin,
                    bundle_root,
                    &canonical,
                    &display_path,
                    components,
                    index,
                    require_directory,
                    active,
                    matched,
                )?;
            }
        }
        return Ok(());
    }

    // Outside the recursive-iterator edge above, glob treats literal empty
    // and `.` path components as the current directory.
    if component.is_empty() || component == "." {
        let next_display_directory =
            if component == "." && !display_directory.as_os_str().is_empty() {
                display_directory.join(".")
            } else {
                display_directory.to_path_buf()
            };
        return walk_confined_pattern(
            origin,
            bundle_root,
            directory,
            &next_display_directory,
            components,
            index + 1,
            require_directory,
            active,
            matched,
        );
    }

    let component_pattern = glob::Pattern::new(component).map_err(|error| {
        format!(
            "invalid pattern component '{}' in '{}': {}",
            component,
            origin.defining_config_path.display(),
            error
        )
    })?;
    for entry in sorted_directory_entries(directory, origin, component)? {
        let file_name = entry.file_name();
        let Some(file_name_text) = file_name.to_str() else {
            continue;
        };
        if !component_pattern.matches(file_name_text) {
            continue;
        }

        let entry_path = entry.path();
        let display_path = display_directory.join(&file_name);
        let canonical = canonicalize_pattern_entry(origin, &entry_path)?;
        ensure_fetched_path(origin, bundle_root, &canonical, &entry_path, component)?;

        if index + 1 == components.len() {
            if !require_directory && canonical.is_file() {
                push_confined_match(&display_path, canonical, matched);
            }
        } else if canonical.is_dir() {
            walk_confined_pattern(
                origin,
                bundle_root,
                &canonical,
                &display_path,
                components,
                index + 1,
                require_directory,
                active,
                matched,
            )?;
        }
    }
    Ok(())
}

fn sorted_directory_entries(
    directory: &Path,
    origin: &DefinitionOrigin,
    pattern: &str,
) -> Result<Vec<fs::DirEntry>, String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "failed to read pattern directory '{}' for '{}' in '{}': {}",
            directory.display(),
            pattern,
            origin.defining_config_path.display(),
            error
        )
    })?;
    let mut entries: Vec<fs::DirEntry> = entries
        .collect::<Result<_, _>>()
        .map_err(|error| format!("failed to read '{}': {}", directory.display(), error))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

fn canonicalize_pattern_entry(origin: &DefinitionOrigin, entry: &Path) -> Result<PathBuf, String> {
    entry.canonicalize().map_err(|error| {
        format!(
            "failed to resolve pattern entry '{}' from '{}': {}",
            entry.display(),
            origin.defining_config_path.display(),
            error
        )
    })
}

fn ensure_fetched_path(
    origin: &DefinitionOrigin,
    bundle_root: &Path,
    canonical: &Path,
    encountered: &Path,
    pattern: &str,
) -> Result<(), String> {
    if canonical.starts_with(bundle_root) {
        return Ok(());
    }
    Err(format!(
        "pattern '{}' in '{}' escapes fetched bundle root '{}' through '{}'",
        pattern,
        origin.defining_config_path.display(),
        bundle_root.display(),
        encountered.display()
    ))
}

fn push_confined_match(
    display_path: &Path,
    executable_path: PathBuf,
    matched: &mut Vec<MatchedPath>,
) {
    matched.push(MatchedPath {
        display_path: display_path.to_string_lossy().replace('\\', "/"),
        executable_path,
    });
}

fn validate_origin_confinement(origin: &DefinitionOrigin) -> Result<(), String> {
    let Some(bundle_root) = &origin.bundle_root else {
        return Ok(());
    };

    if !origin.base_dir.starts_with(bundle_root)
        || !origin.defining_config_path.starts_with(bundle_root)
    {
        return Err(format!(
            "definition '{}' escapes fetched bundle root '{}'",
            origin.defining_config_path.display(),
            bundle_root.display()
        ));
    }
    Ok(())
}

fn validate_confined_pattern(pattern: &str, origin: &DefinitionOrigin) -> Result<(), String> {
    if origin.bundle_root.is_none() {
        return Ok(());
    }

    let path = Path::new(pattern);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "pattern '{}' in '{}' escapes its fetched bundle root",
            pattern,
            origin.defining_config_path.display()
        ));
    }
    Ok(())
}

pub fn expand_rule_files(workdir: &str, patterns: &[String]) -> Result<Vec<String>, String> {
    if patterns.is_empty() {
        return Ok(vec![]);
    }

    let mut positive = vec![];
    let mut negative = vec![];

    for p in patterns {
        let s = p.trim();
        if s.is_empty() {
            continue;
        }
        if s.starts_with('!') {
            negative.push(s[1..].to_string());
        } else {
            positive.push(s.to_string());
        }
    }

    if positive.is_empty() {
        return Ok(vec![]);
    }

    let mut included: HashSet<String> = HashSet::new();

    for pat in &positive {
        let glob_path = Path::new(workdir).join(pat);
        if let Ok(matches) = glob::glob(glob_path.to_string_lossy().as_ref()) {
            for entry in matches.flatten() {
                if entry.is_file() {
                    if let Ok(rel) = entry.strip_prefix(workdir) {
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        included.insert(rel_str);
                    }
                }
            }
        }
    }

    for pat in &negative {
        let glob_path = Path::new(workdir).join(pat.trim());
        if let Ok(matches) = glob::glob(glob_path.to_string_lossy().as_ref()) {
            for entry in matches.flatten() {
                if let Ok(rel) = entry.strip_prefix(workdir) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    included.remove(&rel_str);
                }
            }
        }
    }

    let mut out: Vec<String> = included.into_iter().collect();
    out.sort();
    Ok(out)
}

pub fn run_rule_file(workdir: &str, rel_path: &str) -> RuleResult {
    let script_path = if rel_path.starts_with("./") {
        PathBuf::from(rel_path)
    } else {
        PathBuf::from(format!("./{}", rel_path))
    };
    run_rule_file_at(Path::new(workdir), rel_path, &script_path)
}

pub(crate) fn run_resolved_rule_file(rule_file: ResolvedRuleFile) -> RuleResult {
    run_rule_file_at(
        &rule_file.origin.base_dir,
        &rule_file.display_path,
        &rule_file.executable_path,
    )
}

fn run_rule_file_at(workdir: &Path, display_path: &str, script_path: &Path) -> RuleResult {
    let rule = Rule {
        name: Some(display_path.to_string()),
        check: Some(display_path.to_string()),
        severity: Some(Severity::Error),
        fix: None,
        hint: None,
        remote: None,
    };

    let output = Command::new("bash")
        .arg(script_path)
        .current_dir(workdir)
        .output();

    match output {
        Ok(out) => {
            let exit_error = if !out.status.success() {
                Some(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("command failed with exit code: {:?}", out.status.code()),
                )) as Box<dyn std::error::Error>)
            } else {
                None
            };
            RuleResult {
                rule,
                err: exit_error,
                stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            }
        }
        Err(e) => RuleResult {
            rule,
            err: Some(Box::new(e) as Box<dyn std::error::Error>),
            stdout: String::new(),
            stderr: String::new(),
        },
    }
}

fn rule_meets_severity(rule: &Rule, min: Severity) -> bool {
    let rule_severity = rule.severity.clone().unwrap_or(DEFAULT_RULE_SEVERITY);
    let normalized = normalize_rule_severity(rule_severity);
    severity_order(normalized) >= severity_order(min)
}

fn normalize_rule_severity(value: Severity) -> Severity {
    value
}

fn normalize_min_severity(value: Severity) -> Severity {
    if value == Severity::Debug {
        Severity::Debug
    } else {
        value
    }
}

fn normalize_fail_severity(value: Severity) -> Severity {
    if value == Severity::Debug || value == Severity::Info || value == Severity::Warning {
        Severity::Warning
    } else {
        value
    }
}

pub fn min_severity(a: Severity, b: Severity) -> Severity {
    let a = normalize_min_severity(a);
    let b = normalize_min_severity(b);
    if severity_order(a) <= severity_order(b) {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolved::SourceIdentity;
    use std::fs;
    use tempfile::TempDir;

    fn executable_rule(name: &str, check: &str) -> Rule {
        Rule {
            name: Some(name.to_string()),
            check: Some(check.to_string()),
            severity: Some(Severity::Error),
            fix: None,
            hint: None,
            remote: None,
        }
    }

    fn test_origin(config_path: PathBuf, bundle_root: Option<PathBuf>) -> DefinitionOrigin {
        let base_dir = config_path.parent().unwrap().to_path_buf();
        DefinitionOrigin {
            source_identity: SourceIdentity::Local {
                root: bundle_root.clone().unwrap_or_else(|| base_dir.clone()),
            },
            defining_config_path: config_path,
            source_relative_config: None,
            base_dir,
            bundle_root,
            revision: None,
        }
    }

    #[test]
    fn test_filter_rules_by_severity() {
        let cfg = Config {
            cache_path: None,
            check_severity: None,
            fail_severity: None,
            preconditions: vec![],
            rules: vec![
                Rule {
                    name: Some("debug".to_string()),
                    check: Some("true".to_string()),
                    severity: Some(Severity::Debug),
                    fix: None,
                    hint: None,
                    remote: None,
                },
                Rule {
                    name: Some("info".to_string()),
                    check: Some("true".to_string()),
                    severity: Some(Severity::Info),
                    fix: None,
                    hint: None,
                    remote: None,
                },
                Rule {
                    name: Some("warn".to_string()),
                    check: Some("true".to_string()),
                    severity: Some(Severity::Warning),
                    fix: None,
                    hint: None,
                    remote: None,
                },
            ],
            patterns: vec![],
        };

        let rules = filter_rules(&cfg, Severity::Warning);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name.as_ref().unwrap(), "warn");
    }

    #[test]
    fn test_rule_result_should_fail() {
        let result = RuleResult {
            rule: Rule {
                name: None,
                check: Some("".to_string()),
                severity: Some(Severity::Warning),
                fix: None,
                hint: None,
                remote: None,
            },
            err: Some(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "boom",
            ))),
            stdout: "".to_string(),
            stderr: "".to_string(),
        };
        assert!(!result.should_fail(Severity::Error));
        assert!(result.should_fail(Severity::Warning));
    }

    #[test]
    fn test_report_aggregates_failures() {
        let results = vec![
            RuleResult {
                rule: Rule {
                    name: Some("warn".to_string()),
                    check: Some("".to_string()),
                    severity: Some(Severity::Warning),
                    fix: None,
                    hint: None,
                    remote: None,
                },
                err: Some(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "boom",
                ))),
                stdout: "".to_string(),
                stderr: "".to_string(),
            },
            RuleResult {
                rule: Rule {
                    name: Some("error".to_string()),
                    check: Some("".to_string()),
                    severity: Some(Severity::Error),
                    fix: None,
                    hint: None,
                    remote: None,
                },
                err: Some(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "boom",
                ))),
                stdout: "".to_string(),
                stderr: "".to_string(),
            },
        ];
        let report = Report {
            rules: results,
            fail_severity: Severity::Error,
        };
        assert!(report.has_failures());
        let failures = report.failures();
        assert_eq!(failures.len(), 1);
    }

    #[test]
    fn test_min_severity() {
        assert_eq!(
            min_severity(Severity::Debug, Severity::Warning),
            Severity::Debug
        );
        assert_eq!(
            min_severity(Severity::Error, Severity::Warning),
            Severity::Warning
        );
    }

    #[test]
    fn test_pattern_only_configuration_executes() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("only.sh"), "echo pattern-only\n").unwrap();
        let config = Config {
            cache_path: None,
            check_severity: None,
            fail_severity: None,
            preconditions: vec![],
            rules: vec![],
            patterns: vec!["*.sh".to_string()],
        };

        let report = diagnose(Options {
            config,
            workdir: temp.path().to_string_lossy().into_owned(),
            min_severity: Severity::Debug,
            fail_severity: Severity::Error,
        })
        .unwrap();

        assert_eq!(report.rules.len(), 1);
        assert_eq!(report.rules[0].name(), "only.sh");
        assert_eq!(report.rules[0].stdout, "pattern-only\n");
        assert!(report.rules[0].success());
    }

    #[test]
    fn test_resolved_rules_and_patterns_use_their_defining_directories() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let child = root.join("nested");
        fs::create_dir(&child).unwrap();
        fs::write(root.join(".checksy.yaml"), "rules: []\n").unwrap();
        fs::write(child.join("child.yaml"), "rules: []\n").unwrap();
        fs::write(root.join("root-marker"), "root\n").unwrap();
        fs::write(child.join("child-marker"), "child\n").unwrap();
        fs::write(root.join("root.sh"), "echo root-pattern\n").unwrap();
        fs::write(root.join("skip.sh"), "echo root-skip\n").unwrap();
        fs::write(root.join("z.sh"), "echo root-z\n").unwrap();
        fs::write(child.join("a.sh"), "echo child-pattern\n").unwrap();
        fs::write(child.join("skip.sh"), "exit 1\n").unwrap();

        let root_origin = test_origin(root.join(".checksy.yaml"), None);
        let child_origin = test_origin(child.join("child.yaml"), None);
        let definition = ResolvedDefinition {
            preconditions: vec![ResolvedRule {
                rule: executable_rule("root-inline", "test -f root-marker"),
                origin: root_origin.clone(),
            }],
            rules: vec![ResolvedRule {
                rule: executable_rule("child-inline", "test -f child-marker"),
                origin: child_origin.clone(),
            }],
            pattern_groups: vec![
                ResolvedPatternGroup {
                    patterns: vec!["*.sh".to_string()],
                    origin: root_origin,
                },
                ResolvedPatternGroup {
                    patterns: vec!["*.sh".to_string(), "!skip.sh".to_string()],
                    origin: child_origin,
                },
            ],
            ..ResolvedDefinition::default()
        };

        let report = diagnose_resolved(ResolvedOptions {
            definition,
            min_severity: Severity::Debug,
            fail_severity: Severity::Error,
        })
        .unwrap();

        let names: Vec<String> = report.rules.iter().map(RuleResult::name).collect();
        assert_eq!(
            names,
            [
                "root-inline",
                "child-inline",
                "root.sh",
                "skip.sh",
                "z.sh",
                "a.sh"
            ]
        );
        assert!(report.rules.iter().all(RuleResult::success));
        assert_eq!(report.rules[2].stdout, "root-pattern\n");
        assert_eq!(report.rules[3].stdout, "root-skip\n");
        assert_eq!(report.rules[4].stdout, "root-z\n");
        assert_eq!(report.rules[5].stdout, "child-pattern\n");
    }

    #[test]
    fn test_fetched_pattern_symlink_escape_fails_before_commands_run() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let bundle = temp.path().join("bundle");
        fs::create_dir(&bundle).unwrap();
        let config_path = bundle.join(".checksy.yaml");
        fs::write(&config_path, "rules: []\n").unwrap();
        let outside = temp.path().join("outside.sh");
        fs::write(&outside, "exit 0\n").unwrap();
        symlink(&outside, bundle.join("escaped.sh")).unwrap();
        let marker = bundle.join("command-ran");
        let origin = test_origin(config_path, Some(bundle.clone()));
        let definition = ResolvedDefinition {
            rules: vec![ResolvedRule {
                rule: executable_rule("must-not-run", "touch command-ran"),
                origin: origin.clone(),
            }],
            pattern_groups: vec![ResolvedPatternGroup {
                patterns: vec!["*.sh".to_string()],
                origin,
            }],
            ..ResolvedDefinition::default()
        };

        let error = diagnose_resolved(ResolvedOptions {
            definition,
            min_severity: Severity::Debug,
            fail_severity: Severity::Error,
        })
        .unwrap_err();

        assert!(error.contains("escapes fetched bundle root"), "{error}");
        assert!(!marker.exists(), "rule ran before pattern preflight failed");
    }

    #[cfg(unix)]
    #[test]
    fn test_fetched_pattern_symlink_with_internal_target_is_allowed() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let bundle = temp.path().join("bundle");
        fs::create_dir(&bundle).unwrap();
        let config_path = bundle.join(".checksy.yaml");
        fs::write(&config_path, "rules: []\n").unwrap();
        fs::write(bundle.join("target.sh"), "echo internal\n").unwrap();
        symlink("target.sh", bundle.join("alias.sh")).unwrap();
        let origin = test_origin(config_path, Some(bundle));

        let files = expand_resolved_rule_files(&[ResolvedPatternGroup {
            patterns: vec!["alias.sh".to_string()],
            origin,
        }])
        .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].display_path, "alias.sh");

        let result = run_resolved_rule_file(files.into_iter().next().unwrap());
        assert!(result.success());
        assert_eq!(result.stdout, "internal\n");
    }

    #[cfg(unix)]
    #[test]
    fn test_fetched_pattern_symlink_directory_escape_fails_without_a_file_match() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let bundle = temp.path().join("bundle");
        let outside = temp.path().join("outside");
        fs::create_dir(&bundle).unwrap();
        fs::create_dir(&outside).unwrap();
        let config_path = bundle.join(".checksy.yaml");
        fs::write(&config_path, "rules: []\n").unwrap();
        symlink(&outside, bundle.join("escape")).unwrap();
        let marker = bundle.join("command-ran");
        let origin = test_origin(config_path, Some(bundle));
        let definition = ResolvedDefinition {
            rules: vec![ResolvedRule {
                rule: executable_rule("must-not-run", "touch command-ran"),
                origin: origin.clone(),
            }],
            pattern_groups: vec![ResolvedPatternGroup {
                patterns: vec!["escape/*.sh".to_string()],
                origin,
            }],
            ..ResolvedDefinition::default()
        };

        let error = diagnose_resolved(ResolvedOptions {
            definition,
            min_severity: Severity::Debug,
            fail_severity: Severity::Error,
        })
        .unwrap_err();

        assert!(error.contains("escapes fetched bundle root"), "{error}");
        assert!(!marker.exists(), "rule ran before pattern preflight failed");
    }

    #[cfg(unix)]
    #[test]
    fn test_fetched_negative_only_pattern_still_preflights_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let bundle = temp.path().join("bundle");
        let outside = temp.path().join("outside");
        fs::create_dir(&bundle).unwrap();
        fs::create_dir(&outside).unwrap();
        let config_path = bundle.join(".checksy.yaml");
        fs::write(&config_path, "rules: []\n").unwrap();
        symlink(&outside, bundle.join("escape")).unwrap();
        let origin = test_origin(config_path, Some(bundle));

        let error = expand_resolved_rule_files(&[ResolvedPatternGroup {
            patterns: vec!["!escape/*.sh".to_string()],
            origin,
        }])
        .unwrap_err();

        assert!(error.contains("escapes fetched bundle root"), "{error}");
    }

    #[test]
    fn test_fetched_directory_only_patterns_never_become_scripts() {
        let temp = TempDir::new().unwrap();
        let bundle = temp.path().join("bundle");
        let nested = bundle.join("nested");
        fs::create_dir_all(&nested).unwrap();
        let config_path = bundle.join(".checksy.yaml");
        fs::write(&config_path, "rules: []\n").unwrap();
        fs::write(bundle.join("root.sh"), "exit 0\n").unwrap();
        fs::write(nested.join("nested.sh"), "exit 0\n").unwrap();
        let origin = test_origin(config_path, Some(bundle));

        let directory_only = expand_resolved_rule_files(&[ResolvedPatternGroup {
            patterns: vec!["**/".to_string(), "nested/**/".to_string()],
            origin: origin.clone(),
        }])
        .unwrap();
        assert!(directory_only.is_empty());

        let terminal_recursive = expand_resolved_rule_files(&[ResolvedPatternGroup {
            patterns: vec!["**".to_string(), "nested/**".to_string()],
            origin: origin.clone(),
        }])
        .unwrap();
        assert!(terminal_recursive.is_empty());

        let recursive_special_components = expand_resolved_rule_files(&[ResolvedPatternGroup {
            patterns: vec!["**//root.sh".to_string(), "**/./root.sh".to_string()],
            origin: origin.clone(),
        }])
        .unwrap();
        assert!(recursive_special_components.is_empty());

        let negated_directory_only = expand_resolved_rule_files(&[ResolvedPatternGroup {
            patterns: vec![
                "**/*.sh".to_string(),
                "!**/".to_string(),
                "!**".to_string(),
                "!nested/**".to_string(),
                "!**//root.sh".to_string(),
                "!**/./root.sh".to_string(),
            ],
            origin,
        }])
        .unwrap();
        let names: Vec<&str> = negated_directory_only
            .iter()
            .map(|file| file.display_path.as_str())
            .collect();
        assert_eq!(names, ["nested/nested.sh", "root.sh"]);
    }

    #[test]
    fn test_fetched_pattern_walker_matches_legacy_glob_file_selection() {
        let temp = TempDir::new().unwrap();
        let bundle = temp.path().join("bundle");
        let nested = bundle.join("nested");
        let deeper = nested.join("deeper");
        fs::create_dir_all(&deeper).unwrap();
        let config_path = bundle.join(".checksy.yaml");
        fs::write(&config_path, "rules: []\n").unwrap();
        fs::write(bundle.join("root.sh"), "exit 0\n").unwrap();
        fs::write(nested.join("a.sh"), "exit 0\n").unwrap();
        fs::write(nested.join("note.txt"), "text\n").unwrap();
        fs::write(deeper.join("b.sh"), "exit 0\n").unwrap();
        let origin = test_origin(config_path, Some(bundle.clone()));

        for pattern in [
            "*.sh",
            "./*.sh",
            "nested/*.sh",
            "nested/./a.sh",
            "nested//*.sh",
            "**/*.sh",
            "*/a.sh",
            "**/**/a.sh",
            "**//root.sh",
            "**/./root.sh",
            "**",
            "nested/**",
            "**/",
            "nested/**/",
        ] {
            let legacy_pattern = format!("{}/{}", bundle.display(), pattern);
            let mut expected: Vec<String> = glob::glob(&legacy_pattern)
                .unwrap()
                .flatten()
                .filter(|path| path.is_file())
                .filter_map(|path| {
                    path.strip_prefix(&bundle)
                        .ok()
                        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                })
                .collect();
            expected.sort();
            expected.dedup();

            let actual = expand_resolved_rule_files(&[ResolvedPatternGroup {
                patterns: vec![pattern.to_string()],
                origin: origin.clone(),
            }])
            .unwrap();
            let actual: Vec<String> = actual.into_iter().map(|file| file.display_path).collect();

            assert_eq!(actual, expected, "pattern {pattern:?}");
        }
    }

    #[test]
    fn test_fetched_internal_dot_components_keep_legacy_negation_keys() {
        let temp = TempDir::new().unwrap();
        let bundle = temp.path().join("bundle");
        let nested = bundle.join("nested");
        fs::create_dir_all(&nested).unwrap();
        let config_path = bundle.join(".checksy.yaml");
        fs::write(&config_path, "rules: []\n").unwrap();
        fs::write(nested.join("run.sh"), "exit 0\n").unwrap();
        let origin = test_origin(config_path, Some(bundle));

        let dotted_positive = expand_resolved_rule_files(&[ResolvedPatternGroup {
            patterns: vec!["*/./run.sh".to_string(), "!nested/run.sh".to_string()],
            origin: origin.clone(),
        }])
        .unwrap();
        assert_eq!(dotted_positive.len(), 1);
        assert_eq!(dotted_positive[0].display_path, "nested/./run.sh");

        let dotted_negative = expand_resolved_rule_files(&[ResolvedPatternGroup {
            patterns: vec!["nested/run.sh".to_string(), "!*/./run.sh".to_string()],
            origin,
        }])
        .unwrap();
        assert_eq!(dotted_negative.len(), 1);
        assert_eq!(dotted_negative[0].display_path, "nested/run.sh");
    }

    #[test]
    fn test_fetched_pattern_parent_traversal_is_rejected() {
        let temp = TempDir::new().unwrap();
        let bundle = temp.path().join("bundle");
        fs::create_dir(&bundle).unwrap();
        let config_path = bundle.join(".checksy.yaml");
        fs::write(&config_path, "rules: []\n").unwrap();
        let origin = test_origin(config_path, Some(bundle));

        let error = expand_resolved_rule_files(&[ResolvedPatternGroup {
            patterns: vec!["../*.sh".to_string()],
            origin,
        }])
        .unwrap_err();

        assert!(error.contains("escapes its fetched bundle root"), "{error}");
    }

    #[test]
    fn test_local_absolute_pattern_outside_base_preserves_legacy_skip_behavior() {
        let temp = TempDir::new().unwrap();
        let local = temp.path().join("local");
        fs::create_dir(&local).unwrap();
        let config_path = local.join(".checksy.yaml");
        fs::write(&config_path, "rules: []\n").unwrap();
        let outside = temp.path().join("outside.sh");
        fs::write(&outside, "exit 0\n").unwrap();
        let origin = test_origin(config_path, None);

        let files = expand_resolved_rule_files(&[ResolvedPatternGroup {
            patterns: vec![outside.to_string_lossy().into_owned()],
            origin,
        }])
        .unwrap();

        assert!(files.is_empty());
    }
}
