use crate::config::{load, resolve_path};
use crate::doctor::{self, Options, Report, RuleResult};
use crate::schema::{Rule, Severity};
use crate::version::VERSION;
use std::io::Write;
use std::path::Path;

const DEFAULT_INIT_CONFIG_FILENAME: &str = ".checksy.config.yaml";
const DEFAULT_INIT_CONFIG_TEMPLATE: &str = r#"# checksy configuration
rules:
  - name: "Example rule"
    severity: error
    check: |
      echo "Replace this with a useful check"
"#;

pub fn run(args: Vec<String>, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    if args.is_empty() || args[0] == "help" || args[0] == "-h" || args[0] == "--help" {
        print_usage(stdout);
        return if args.is_empty() { 1 } else { 0 };
    }

    let (globals, remaining) = match parse_global_flags(&args) {
        Ok((g, r)) => (g, r),
        Err(e) => {
            writeln!(stderr, "{}", e).ok();
            return 2;
        }
    };

    if remaining.is_empty() {
        print_usage(stdout);
        return 1;
    }

    let cmd = &remaining[0];
    let cmd_args = &remaining[1..];

    match cmd.as_str() {
        "check" => run_check(cmd_args.to_vec(), globals, stdout, stderr),
        "diagnose" => run_diagnose(cmd_args.to_vec(), globals, stdout, stderr),
        "init" => run_init(cmd_args.to_vec(), globals, stdout, stderr),
        "schema" => run_schema(cmd_args.to_vec(), stdout, stderr),
        "version" | "--version" => {
            writeln!(stdout, "checksy {}", VERSION).ok();
            0
        }
        _ => {
            writeln!(stderr, "Unknown command '{}'\n", cmd).ok();
            print_usage(stdout);
            2
        }
    }
}

#[derive(Debug, Default)]
struct GlobalFlags {
    config_path: Option<String>,
    stdin_config: bool,
}

fn parse_global_flags(args: &[String]) -> Result<(GlobalFlags, Vec<String>), String> {
    let mut globals = GlobalFlags::default();
    let mut remaining = vec![];

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--config" | "-config" => {
                if i + 1 >= args.len() {
                    return Err("--config flag requires a value".to_string());
                }
                globals.config_path = Some(args[i + 1].clone());
                i += 2;
                continue;
            }
            "--stdin-config" => {
                globals.stdin_config = true;
                i += 1;
                continue;
            }
            _ if arg.starts_with("--config=") => {
                globals.config_path = Some(arg.trim_start_matches("--config=").to_string());
                i += 1;
                continue;
            }
            _ if arg.starts_with("-config=") => {
                globals.config_path = Some(arg.trim_start_matches("-config=").to_string());
                i += 1;
                continue;
            }
            _ => {}
        }
        remaining.push(arg.clone());
        i += 1;
    }

    Ok((globals, remaining))
}

fn run_diagnose(
    args: Vec<String>,
    globals: GlobalFlags,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let _ = writeln!(
        stderr,
        "⚠️  \"checksy diagnose\" is deprecated, please use \"checksy check\" instead"
    );
    run_check(args, globals, stdout, stderr)
}

fn run_check(
    args: Vec<String>,
    globals: GlobalFlags,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut config_path = None;
    let mut no_fail = false;
    let mut check_severity = None;
    let mut fail_severity = None;
    let mut apply_fixes = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                if i + 1 >= args.len() {
                    writeln!(stderr, "--config requires a value").ok();
                    return 2;
                }
                config_path = Some(args[i + 1].clone());
                i += 2;
            }
            "--no-fail" => {
                no_fail = true;
                i += 1;
            }
            "--check-severity" | "--cs" => {
                if i + 1 >= args.len() {
                    writeln!(stderr, "{} requires a value", args[i]).ok();
                    return 2;
                }
                check_severity = Some(args[i + 1].clone());
                i += 2;
            }
            "--fail-severity" | "--fs" => {
                if i + 1 >= args.len() {
                    writeln!(stderr, "{} requires a value", args[i]).ok();
                    return 2;
                }
                fail_severity = Some(args[i + 1].clone());
                i += 2;
            }
            "--fix" => {
                apply_fixes = true;
                i += 1;
            }
            _ => {
                writeln!(stderr, "Unknown flag: {}", args[i]).ok();
                return 2;
            }
        }
    }

    let config_path = config_path.or(globals.config_path).unwrap_or_else(|| {
        if globals.stdin_config {
            "-".to_string()
        } else {
            String::new()
        }
    });
    let resolved = match resolve_path(&config_path) {
        Ok(Some(p)) => p,
        Ok(None) => {
            writeln!(stderr, "no configuration file found; specify --config or add .checksy.yaml to the workspace").ok();
            return 2;
        }
        Err(e) => {
            writeln!(stderr, "{}", e).ok();
            return 2;
        }
    };

    let abs_config_path = if resolved == "-" {
        "-".to_string()
    } else {
        match std::fs::canonicalize(&resolved) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(e) => {
                writeln!(stderr, "unable to resolve config path: {}", e).ok();
                return 2;
            }
        }
    };

    let cfg = match load(&abs_config_path) {
        Ok(c) => c,
        Err(e) => {
            writeln!(stderr, "failed to load config '{}': {}", abs_config_path, e).ok();
            return 2;
        }
    };

    let check_severity = if let Some(ref s) = check_severity {
        match parse_severity(s) {
            Ok(sev) => sev,
            Err(e) => {
                writeln!(stderr, "{}", e).ok();
                return 2;
            }
        }
    } else if let Some(s) = cfg.check_severity {
        s
    } else {
        Severity::Debug
    };

    let fail_severity = if let Some(ref s) = fail_severity {
        match parse_severity(s) {
            Ok(sev) => sev,
            Err(e) => {
                writeln!(stderr, "{}", e).ok();
                return 2;
            }
        }
    } else if let Some(s) = cfg.fail_severity {
        s
    } else {
        Severity::Error
    };

    let min_severity = doctor::min_severity(check_severity, fail_severity);

    let workdir = Path::new(&abs_config_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    let opts = Options {
        config: cfg,
        workdir,
        min_severity,
        fail_severity,
    };

    let report = if apply_fixes {
        match check_with_fixes(opts, stdout, stderr) {
            Ok(r) => r,
            Err(e) => {
                writeln!(stderr, "check failed: {}", e).ok();
                return 2;
            }
        }
    } else {
        match doctor::diagnose(opts) {
            Ok(r) => r,
            Err(e) => {
                writeln!(stderr, "check failed: {}", e).ok();
                return 2;
            }
        }
    };

    if !apply_fixes {
        print_report_results(&report, stdout, stderr);
    }

    summarize_report(&report, no_fail, stdout)
}

fn run_init(
    args: Vec<String>,
    _globals: GlobalFlags,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    if !args.is_empty() {
        writeln!(
            stderr,
            "init does not accept positional arguments: {:?}",
            args
        )
        .ok();
        return 2;
    }

    let path = DEFAULT_INIT_CONFIG_FILENAME;

    if let Err(e) = write_config_template(path) {
        writeln!(stderr, "init failed: {}", e).ok();
        return 2;
    }

    writeln!(stdout, "Created {}", path).ok();
    0
}

fn write_config_template(path: &str) -> Result<(), String> {
    let path = if path.is_empty() {
        DEFAULT_INIT_CONFIG_FILENAME
    } else {
        path
    };

    let p = Path::new(path);
    if p.exists() {
        if p.is_dir() {
            return Err(format!("{} is a directory", path));
        }
        return Err(format!("{} already exists", path));
    }

    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() && parent.as_os_str() != std::path::Path::new(".") {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create config directory: {}", e))?;
        }
    }

    let mut content = DEFAULT_INIT_CONFIG_TEMPLATE.to_string();
    if !content.ends_with('\n') {
        content.push('\n');
    }
    std::fs::write(path, content).map_err(|e| format!("write config: {}", e))?;

    Ok(())
}

fn run_schema(_args: Vec<String>, stdout: &mut dyn Write, _stderr: &mut dyn Write) -> i32 {
    let schema_json = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "checksy configuration",
  "type": "object",
  "properties": {
    "checkSeverity": {
      "type": "string",
      "enum": ["debug", "info", "warn", "error"]
    },
    "failSeverity": {
      "type": "string",
      "enum": ["debug", "info", "warn", "error"]
    },
    "preconditions": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["check"],
        "properties": {
          "name": { "type": "string" },
          "check": { "type": "string" },
          "severity": { "type": "string", "enum": ["debug", "info", "warn", "error"], "default": "error" },
          "fix": { "type": "string" },
          "hint": { "type": "string" }
        }
      }
    },
    "rules": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["check"],
        "properties": {
          "name": { "type": "string" },
          "check": { "type": "string" },
          "severity": { "type": "string", "enum": ["debug", "info", "warn", "error"], "default": "error" },
          "fix": { "type": "string" },
          "hint": { "type": "string" }
        }
      }
    },
    "patterns": {
      "type": "array",
      "items": { "type": "string" }
    }
  },
  "required": ["rules"]
}"#;

    writeln!(stdout, "{}", schema_json).ok();
    0
}

fn print_usage(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "checksy - inspect and troubleshoot development environments"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Usage:");
    let _ = writeln!(stdout, "  checksy [global flags] <command> [command flags]");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Global Flags:");
    let _ = writeln!(
        stdout,
        "  --config string   path to config file (defaults to .checksy.yaml)"
    );
    let _ = writeln!(stdout, "  --stdin-config    read config from stdin");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Available Commands:");
    let _ = writeln!(stdout, "  check      Run checks for config-defined rules");
    let _ = writeln!(
        stdout,
        "  diagnose   Run checks (deprecated, use 'check' instead)"
    );
    let _ = writeln!(
        stdout,
        "  schema     Print the JSON schema for configuration file"
    );
    let _ = writeln!(stdout, "  version    Print the current build version");
    let _ = writeln!(stdout, "  help       Show this message");
}

fn parse_severity(value: &str) -> Result<Severity, String> {
    match value.to_lowercase().trim() {
        "" | "debug" => Ok(Severity::Debug),
        "info" => Ok(Severity::Info),
        "warning" | "warn" => Ok(Severity::Warning),
        "error" => Ok(Severity::Error),
        _ => Err(format!(
            "invalid severity '{}': must be one of debug, info, warn, error",
            value
        )),
    }
}

fn print_report_results(report: &Report, stdout: &mut dyn Write, stderr: &mut dyn Write) {
    let fail_severity =
        if report.fail_severity == Severity::Debug || report.fail_severity == Severity::Info {
            Severity::Warning
        } else {
            report.fail_severity
        };

    for result in &report.rules {
        print_rule_outcome(result, fail_severity, stdout, stderr);
    }
}

fn print_rule_status(
    result: &RuleResult,
    icon: &str,
    include_output: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) {
    let _ = writeln!(stdout, "{} {}", icon, result.name());
    if !include_output || result.success() {
        return;
    }
    if !result.stdout.is_empty() {
        let _ = writeln!(stderr, "{} stdout:\n{}", result.name(), result.stdout);
    }
    if !result.stderr.is_empty() {
        let _ = writeln!(stderr, "{} stderr:\n{}", result.name(), result.stderr);
    }
    if result.stdout.is_empty() && result.stderr.is_empty() {
        if let Some(ref err) = result.err {
            let _ = writeln!(stderr, "{} error: {}", result.name(), err);
        }
    }
    if let Some(ref hint) = result.rule.hint {
        let _ = writeln!(stderr, "{} hint: {}", result.name(), hint);
    }
}

fn print_rule_success(result: &RuleResult, stdout: &mut dyn Write, stderr: &mut dyn Write) {
    print_rule_status(result, "✅", false, stdout, stderr);
}

fn print_rule_failure(result: &RuleResult, stdout: &mut dyn Write, stderr: &mut dyn Write) {
    print_rule_status(result, "❌", true, stdout, stderr);
}

fn print_rule_warning(result: &RuleResult, stdout: &mut dyn Write, stderr: &mut dyn Write) {
    print_rule_status(result, "⚠️ ", true, stdout, stderr);
}

fn print_rule_outcome(
    result: &RuleResult,
    fail_severity: Severity,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) {
    if result.success() {
        print_rule_success(result, stdout, stderr);
        return;
    }
    if result.should_fail(fail_severity) {
        print_rule_failure(result, stdout, stderr);
        return;
    }
    print_rule_warning(result, stdout, stderr);
}

fn summarize_report(report: &Report, no_fail: bool, stdout: &mut dyn Write) -> i32 {
    if !report.has_failures() {
        let _ = writeln!(stdout, "😎 All rules validated");
        return 0;
    }

    let failures = report.failures();
    let _ = writeln!(stdout, "😭 {} rules failed validation", failures.len());
    for failure in &failures {
        let _ = writeln!(stdout, "- {}", failure.name());
    }

    if no_fail {
        return 0;
    }

    3
}

fn check_with_fixes(
    opts: Options,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<Report, String> {
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
    let mut results = vec![];

    let preconditions = doctor::filter_preconditions(&opts.config, opts.min_severity);
    for rule in preconditions {
        let result = doctor::run_rule(rule.clone(), workdir);
        if result.success() {
            print_rule_success(&result, stdout, stderr);
            results.push(result);
            continue;
        }

        if rule.fix.as_ref().map(|s| s.trim()).unwrap_or("").is_empty() {
            print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
            results.push(result);
            continue;
        }

        print_rule_status(&result, "⚠️ ", false, stdout, stderr);

        let fix_rule = Rule {
            name: Some(format!("{} fix", rule_display_name(&rule))),
            check: rule.fix.clone().unwrap_or_default(),
            severity: rule.severity.clone(),
            fix: None,
            hint: rule.hint.clone(),
        };
        let fix_result = doctor::run_rule(fix_rule, workdir);
        if !fix_result.success() {
            print_rule_failure(&fix_result, stdout, stderr);
            print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
            results.push(result);
            continue;
        }

        print_rule_success(&fix_result, stdout, stderr);

        let result = doctor::run_rule(rule, workdir);
        print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
        results.push(result);
    }

    let rules = doctor::filter_rules(&opts.config, opts.min_severity);
    for rule in rules {
        let result = doctor::run_rule(rule.clone(), workdir);
        if result.success() {
            print_rule_success(&result, stdout, stderr);
            results.push(result);
            continue;
        }

        if rule.fix.as_ref().map(|s| s.trim()).unwrap_or("").is_empty() {
            print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
            results.push(result);
            continue;
        }

        print_rule_status(&result, "⚠️ ", false, stdout, stderr);

        let fix_rule = Rule {
            name: Some(format!("{} fix", rule_display_name(&rule))),
            check: rule.fix.clone().unwrap_or_default(),
            severity: rule.severity.clone(),
            fix: None,
            hint: rule.hint.clone(),
        };
        let fix_result = doctor::run_rule(fix_rule, workdir);
        if !fix_result.success() {
            print_rule_failure(&fix_result, stdout, stderr);
            print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
            results.push(result);
            continue;
        }

        print_rule_success(&fix_result, stdout, stderr);

        let result = doctor::run_rule(rule, workdir);
        print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
        results.push(result);
    }

    let file_paths = doctor::expand_rule_files(workdir, &opts.config.patterns)?;
    for rel_path in file_paths {
        let result = doctor::run_rule_file(workdir, &rel_path);
        print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
        results.push(result);
    }

    Ok(Report {
        rules: results,
        fail_severity: opts.fail_severity,
    })
}

fn rule_display_name(rule: &Rule) -> String {
    rule.name
        .clone()
        .unwrap_or_else(|| rule.check.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_severity() {
        assert_eq!(parse_severity("debug").unwrap(), Severity::Debug);
        assert_eq!(parse_severity("info").unwrap(), Severity::Info);
        assert_eq!(parse_severity("warn").unwrap(), Severity::Warning);
        assert_eq!(parse_severity("warning").unwrap(), Severity::Warning);
        assert_eq!(parse_severity("error").unwrap(), Severity::Error);
        assert_eq!(parse_severity("").unwrap(), Severity::Debug);
        assert!(parse_severity("invalid").is_err());
    }

    #[test]
    fn test_rule_display_name() {
        let rule = Rule {
            name: None,
            check: "echo hi".to_string(),
            severity: None,
            fix: None,
            hint: None,
        };
        assert!(rule_display_name(&rule).contains("echo hi"));

        let rule = Rule {
            name: Some("custom".to_string()),
            check: "echo hi".to_string(),
            severity: None,
            fix: None,
            hint: None,
        };
        assert_eq!(rule_display_name(&rule), "custom");
    }
}
