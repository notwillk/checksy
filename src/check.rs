use crate::schema::{severity_order, Config, Rule, Severity};
use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Stdio};

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
    if opts.config.rules.is_empty() && opts.config.preconditions.is_empty() {
        return Ok(Report {
            rules: vec![],
            fail_severity: opts.fail_severity,
        });
    }

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

    let file_paths = expand_rule_files(workdir, &opts.config.patterns)?;
    for rel_path in file_paths {
        results.push(run_rule_file(workdir, &rel_path));
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

pub fn run_rule(rule: Rule, workdir: &str) -> RuleResult {
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
    let rule = Rule {
        name: Some(rel_path.to_string()),
        check: Some(rel_path.to_string()),
        severity: Some(Severity::Error),
        fix: None,
        hint: None,
        remote: None,
    };

    let script_path = if rel_path.starts_with("./") {
        rel_path.to_string()
    } else {
        format!("./{}", rel_path)
    };

    let output = Command::new("bash")
        .arg(&script_path)
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
}
