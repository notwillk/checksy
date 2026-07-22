use crate::process_runner::{self, ProcessError, ProcessLimits};
use crate::schema::{severity_order, Config, Rule, Severity};
use std::collections::HashSet;
use std::fmt;
use std::path::Path;
use std::process::Command;

const DEFAULT_RULE_SEVERITY: Severity = Severity::Error;

#[derive(Debug, Clone)]
pub struct Options {
    pub config: Config,
    pub workdir: String,
    pub min_severity: Severity,
    pub fail_severity: Severity,
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

/// A command-runner failure, as distinct from an ordinary nonzero check.
///
/// This stays private to the crate so the public 0.x reporting API remains
/// source-compatible apart from the documented `Rule::timeout` field.
#[derive(Debug)]
pub(crate) struct ExecutionError {
    command_name: String,
    source: Box<ExecutionFailure>,
}

impl ExecutionError {
    fn new(command_name: String, source: ProcessError) -> Self {
        Self {
            command_name,
            source: Box::new(ExecutionFailure::Process(source)),
        }
    }

    fn invalid_timeout(command_name: String, error: String) -> Self {
        Self {
            command_name,
            source: Box::new(ExecutionFailure::InvalidTimeout(error)),
        }
    }

    pub(crate) fn command_name(&self) -> &str {
        &self.command_name
    }

    pub(crate) fn stdout(&self) -> String {
        self.process_error()
            .and_then(ProcessError::output)
            .map(|output| output.stdout.render_lossy())
            .unwrap_or_default()
    }

    pub(crate) fn stderr(&self) -> String {
        self.process_error()
            .and_then(ProcessError::output)
            .map(|output| output.stderr.render_lossy())
            .unwrap_or_default()
    }

    pub(crate) fn interrupted_signal(&self) -> Option<i32> {
        match self.process_error() {
            Some(ProcessError::Interrupted { signal, .. }) => Some(*signal),
            _ => None,
        }
    }

    pub(crate) fn restore_signal_handlers(&mut self) -> std::io::Result<()> {
        match self.source.as_mut() {
            ExecutionFailure::Process(error) => error.restore_signal_handlers(),
            ExecutionFailure::InvalidTimeout(_) => Ok(()),
        }
    }

    fn process_error(&self) -> Option<&ProcessError> {
        match self.source.as_ref() {
            ExecutionFailure::Process(error) => Some(error),
            ExecutionFailure::InvalidTimeout(_) => None,
        }
    }

    fn into_legacy_result(mut self, rule: Rule) -> RuleResult {
        let stdout = self.stdout();
        let stderr = self.stderr();
        let _ = self.restore_signal_handlers();
        RuleResult {
            rule,
            err: Some(Box::new(self)),
            stdout,
            stderr,
        }
    }
}

#[derive(Debug)]
enum ExecutionFailure {
    Process(ProcessError),
    InvalidTimeout(String),
}

impl fmt::Display for ExecutionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Process(error) => error.fmt(formatter),
            Self::InvalidTimeout(error) => formatter.write_str(error),
        }
    }
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.command_name, self.source)
    }
}

impl std::error::Error for ExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.process_error()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}

#[derive(Debug)]
pub(crate) enum DiagnoseError {
    Configuration(String),
    Execution(ExecutionError),
}

impl DiagnoseError {
    pub(crate) fn execution(&self) -> Option<&ExecutionError> {
        match self {
            Self::Execution(error) => Some(error),
            Self::Configuration(_) => None,
        }
    }

    pub(crate) fn execution_mut(&mut self) -> Option<&mut ExecutionError> {
        match self {
            Self::Execution(error) => Some(error),
            Self::Configuration(_) => None,
        }
    }
}

impl fmt::Display for DiagnoseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(error) => formatter.write_str(error),
            Self::Execution(error) => error.fmt(formatter),
        }
    }
}

impl From<ExecutionError> for DiagnoseError {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
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
        normalize_rule_severity(self.rule.severity.unwrap_or(DEFAULT_RULE_SEVERITY))
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
    diagnose_supervised(opts).map_err(|error| error.to_string())
}

pub(crate) fn diagnose_supervised(opts: Options) -> Result<Report, DiagnoseError> {
    opts.config
        .validate()
        .map_err(DiagnoseError::Configuration)?;

    let workdir = if opts.workdir.is_empty() {
        "."
    } else {
        &opts.workdir
    };

    let mut results = run_preconditions_supervised(&opts, workdir)?;

    let rules = filter_rules(&opts.config, opts.min_severity);
    for rule in rules {
        results.push(run_rule_supervised(rule, workdir)?);
    }

    let file_paths =
        expand_rule_files(workdir, &opts.config.patterns).map_err(DiagnoseError::Configuration)?;
    for rel_path in file_paths {
        results.push(run_rule_file_supervised(workdir, &rel_path)?);
    }

    Ok(Report {
        rules: results,
        fail_severity: opts.fail_severity,
    })
}

fn run_preconditions_supervised(
    opts: &Options,
    workdir: &str,
) -> Result<Vec<RuleResult>, ExecutionError> {
    let preconditions = filter_preconditions(&opts.config, opts.min_severity);
    preconditions
        .into_iter()
        .map(|rule| run_rule_supervised(rule, workdir))
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

pub fn run_rule(rule: Rule, workdir: &str) -> RuleResult {
    let legacy_rule = rule.clone();
    match run_rule_supervised(rule, workdir) {
        Ok(result) => result,
        Err(error) => error.into_legacy_result(legacy_rule),
    }
}

pub(crate) fn run_rule_supervised(rule: Rule, workdir: &str) -> Result<RuleResult, ExecutionError> {
    let script = rule.check.clone().unwrap_or_else(|| "true".to_string());
    let script = if script.ends_with('\n') {
        script
    } else {
        format!("{}\n", script)
    };

    let command_name = rule
        .name
        .clone()
        .unwrap_or_else(|| rule.check.clone().unwrap_or_default());
    let timeout = rule
        .effective_timeout()
        .map_err(|error| ExecutionError::invalid_timeout(command_name.clone(), error))?;
    let mut command = Command::new("bash");
    command.current_dir(workdir).arg("-c").arg(&script);

    let output = process_runner::run(
        command,
        ProcessLimits {
            timeout,
            ..ProcessLimits::default()
        },
    )
    .map_err(|error| ExecutionError::new(command_name, error))?;

    Ok(completed_rule_result(rule, output))
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
        if let Some(negated) = s.strip_prefix('!') {
            negative.push(negated.to_string());
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
    let rule = Rule {
        name: Some(rel_path.to_string()),
        check: Some(rel_path.to_string()),
        severity: Some(Severity::Error),
        fix: None,
        hint: None,
        remote: None,
        timeout: None,
    };

    let legacy_rule = rule.clone();
    match run_rule_file_with_rule(workdir, rel_path, rule) {
        Ok(result) => result,
        Err(error) => error.into_legacy_result(legacy_rule),
    }
}

pub(crate) fn run_rule_file_supervised(
    workdir: &str,
    rel_path: &str,
) -> Result<RuleResult, ExecutionError> {
    let rule = Rule {
        name: Some(rel_path.to_string()),
        check: Some(rel_path.to_string()),
        severity: Some(Severity::Error),
        fix: None,
        hint: None,
        remote: None,
        timeout: None,
    };
    run_rule_file_with_rule(workdir, rel_path, rule)
}

fn run_rule_file_with_rule(
    workdir: &str,
    rel_path: &str,
    rule: Rule,
) -> Result<RuleResult, ExecutionError> {
    let script_path = if rel_path.starts_with("./") {
        rel_path.to_string()
    } else {
        format!("./{}", rel_path)
    };

    let mut command = Command::new("bash");
    command.arg(&script_path).current_dir(workdir);
    let output = process_runner::run(command, ProcessLimits::default())
        .map_err(|error| ExecutionError::new(rel_path.to_string(), error))?;
    Ok(completed_rule_result(rule, output))
}

fn completed_rule_result(rule: Rule, output: process_runner::CompletedProcess) -> RuleResult {
    let exit_error = if !output.status.success() {
        Some(Box::new(std::io::Error::other(format!(
            "command failed with exit code: {:?}",
            output.status.code()
        ))) as Box<dyn std::error::Error>)
    } else {
        None
    };
    RuleResult {
        rule,
        err: exit_error,
        stdout: output.stdout.render_lossy(),
        stderr: output.stderr.render_lossy(),
    }
}

fn rule_meets_severity(rule: &Rule, min: Severity) -> bool {
    let rule_severity = rule.severity.unwrap_or(DEFAULT_RULE_SEVERITY);
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
                    timeout: None,
                },
                Rule {
                    name: Some("info".to_string()),
                    check: Some("true".to_string()),
                    severity: Some(Severity::Info),
                    fix: None,
                    hint: None,
                    remote: None,
                    timeout: None,
                },
                Rule {
                    name: Some("warn".to_string()),
                    check: Some("true".to_string()),
                    severity: Some(Severity::Warning),
                    fix: None,
                    hint: None,
                    remote: None,
                    timeout: None,
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
                timeout: None,
            },
            err: Some(Box::new(std::io::Error::other("boom"))),
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
                    timeout: None,
                },
                err: Some(Box::new(std::io::Error::other("boom"))),
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
                    timeout: None,
                },
                err: Some(Box::new(std::io::Error::other("boom"))),
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
    fn diagnose_validates_programmatic_configs_before_executing_any_rule() {
        let directory = tempfile::tempdir().unwrap();
        let config = Config {
            cache_path: None,
            check_severity: None,
            fail_severity: None,
            preconditions: vec![],
            rules: vec![
                Rule {
                    name: Some("must not run".to_string()),
                    check: Some(": > unexpected-marker".to_string()),
                    severity: Some(Severity::Error),
                    fix: None,
                    hint: None,
                    remote: None,
                    timeout: None,
                },
                Rule {
                    name: Some("invalid later timeout".to_string()),
                    check: Some("true".to_string()),
                    severity: Some(Severity::Error),
                    fix: None,
                    hint: None,
                    remote: None,
                    timeout: Some("0s".to_string()),
                },
            ],
            patterns: vec![],
        };

        let result = diagnose_supervised(Options {
            config,
            workdir: directory.path().to_string_lossy().into_owned(),
            min_severity: Severity::Debug,
            fail_severity: Severity::Error,
        });

        assert!(matches!(result, Err(DiagnoseError::Configuration(_))));
        assert!(!directory.path().join("unexpected-marker").exists());
    }
}
