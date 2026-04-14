use crate::cache::CacheManager;
use crate::check::{self, Options, Report, RuleResult};
use crate::config::{load, parse_git_remote, resolve_path};
use crate::git::GitCache;
use crate::schema::{Config, Rule, Severity};
use crate::version::VERSION;
use std::collections::HashSet;
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
        "install" => run_install(cmd_args.to_vec(), globals, stdout, stderr),
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

    let cfg = match load_with_fix(&abs_config_path, apply_fixes, stdout, stderr) {
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

    let min_severity = check::min_severity(check_severity, fail_severity);

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
        match check::diagnose(opts) {
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

/// Load config, optionally fixing missing git remotes
fn load_with_fix(
    abs_config_path: &str,
    apply_fixes: bool,
    stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> Result<Config, String> {
    match load(abs_config_path) {
        Ok(c) => Ok(c),
        Err(e) => {
            // Check if this is a "not cached" error and --fix is enabled
            if !apply_fixes || !e.contains("git remote not cached") {
                return Err(e);
            }

            // Need to cache git remotes
            writeln!(stdout, "🔧 Caching missing git remotes...").ok();

            let config_dir = if abs_config_path == "-" {
                std::path::PathBuf::from(".")
            } else {
                std::path::Path::new(abs_config_path)
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
            };

            // Load config without expanding to collect git remotes
            let cfg = match load_without_remote_expansion(std::path::Path::new(abs_config_path)) {
                Ok(c) => c,
                Err(e2) => {
                    return Err(format!("{} (and failed to parse for fix: {})", e, e2));
                }
            };

            // Collect all git remotes
            let git_remotes =
                match collect_git_remotes_recursive(&cfg, &config_dir, &cfg.cache_path) {
                    Ok(remotes) => remotes,
                    Err(e2) => {
                        return Err(format!("{} (and failed to collect remotes: {})", e, e2));
                    }
                };

            if git_remotes.is_empty() {
                return Err(e); // No git remotes to fix, return original error
            }

            // Cache each remote
            let cache_mgr = CacheManager::new(&config_dir, cfg.cache_path.as_deref());

            for (i, (repo, ref_)) in git_remotes.iter().enumerate() {
                if cache_mgr.is_cached(repo, ref_) {
                    continue;
                }

                let _ = write!(
                    stdout,
                    "  [{}/{}] {}#{} ",
                    i + 1,
                    git_remotes.len(),
                    repo,
                    ref_
                );

                let dest = cache_mgr.ref_cache_path(repo, ref_);
                match GitCache::shallow_clone(repo, ref_, &dest) {
                    Ok(_) => {
                        let _ = writeln!(stdout, "✓");
                    }
                    Err(e2) => {
                        let _ = writeln!(stdout, "✗");
                        return Err(format!("failed to cache {}#{}: {}", repo, ref_, e2));
                    }
                }
            }

            writeln!(stdout, "✅ Git remotes cached, retrying...").ok();

            // Retry loading the config
            load(abs_config_path)
        }
    }
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

fn run_install(
    args: Vec<String>,
    globals: GlobalFlags,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    // Parse flags
    let mut prune = false;
    for arg in &args {
        match arg.as_str() {
            "--prune" => prune = true,
            _ => {
                writeln!(stderr, "Unknown install flag: {}", arg).ok();
                return 2;
            }
        }
    }

    // Resolve config path
    let config_path = globals.config_path.unwrap_or_else(|| {
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
        writeln!(stderr, "install cannot be used with stdin config").ok();
        return 2;
    } else {
        match std::fs::canonicalize(&resolved) {
            Ok(p) => p,
            Err(e) => {
                writeln!(stderr, "unable to resolve config path: {}", e).ok();
                return 2;
            }
        }
    };

    let config_dir = abs_config_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    // Load config (without expanding remotes - we just need to find git remotes)
    let cfg = match load_without_remote_expansion(&abs_config_path) {
        Ok(c) => c,
        Err(e) => {
            writeln!(
                stderr,
                "failed to load config '{}': {}",
                abs_config_path.display(),
                e
            )
            .ok();
            return 2;
        }
    };

    // Collect all git remotes recursively
    let git_remotes = match collect_git_remotes_recursive(&cfg, &config_dir, &cfg.cache_path) {
        Ok(remotes) => remotes,
        Err(e) => {
            writeln!(stderr, "failed to collect git remotes: {}", e).ok();
            return 2;
        }
    };

    if git_remotes.is_empty() {
        writeln!(stdout, "No git remotes to cache").ok();
        return 0;
    }

    // Show spinner
    let _ = writeln!(stdout, "📦 Caching {} git remote(s)...", git_remotes.len());

    // Cache each remote
    let cache_mgr = CacheManager::new(&config_dir, cfg.cache_path.as_deref());

    for (i, (repo, ref_)) in git_remotes.iter().enumerate() {
        let _ = write!(
            stdout,
            "  [{}/{}] {}#{} ",
            i + 1,
            git_remotes.len(),
            repo,
            ref_
        );

        if cache_mgr.is_cached(repo, ref_) {
            let cache_path = cache_mgr.ref_cache_path(repo, ref_);

            // Check local SHA
            let local_sha = match GitCache::get_local_sha(&cache_path) {
                Ok(sha) => sha,
                Err(e) => {
                    let _ = writeln!(stdout, "✗");
                    let _ = writeln!(
                        stderr,
                        "Failed to read local cache for {}#{}: {}",
                        repo, ref_, e
                    );
                    return 2;
                }
            };

            // Get remote SHA
            let remote_sha = match GitCache::get_remote_sha(repo, ref_) {
                Ok(sha) => sha,
                Err(e) => {
                    let _ = writeln!(stdout, "✗");
                    let _ = writeln!(
                        stderr,
                        "Failed to check remote for {}#{}: {}",
                        repo, ref_, e
                    );
                    return 2;
                }
            };

            // Compare SHAs
            if local_sha == remote_sha {
                let _ = writeln!(stdout, "✓ (already cached)");
                continue;
            }

            // SHAs differ - need to update
            let _ = writeln!(stdout, "↑ updating...");

            // Remove old cache before re-cloning
            if let Err(e) = std::fs::remove_dir_all(&cache_path) {
                let _ = writeln!(stdout, "✗");
                let _ = writeln!(
                    stderr,
                    "Failed to remove old cache for {}#{}: {}",
                    repo, ref_, e
                );
                return 2;
            }

            // Fall through to re-clone
        }

        let dest = cache_mgr.ref_cache_path(repo, ref_);
        match GitCache::shallow_clone(repo, ref_, &dest) {
            Ok(_) => {
                let _ = writeln!(stdout, "✓");
            }
            Err(e) => {
                let _ = writeln!(stdout, "✗");
                let _ = writeln!(stderr, "Failed to cache {}#{}: {}", repo, ref_, e);
                return 2;
            }
        }
    }

    let _ = writeln!(stdout, "✅ All remotes cached");

    // Prune if requested
    if prune {
        let used_set: HashSet<(String, String)> = git_remotes
            .into_iter()
            .map(|(repo, ref_)| (CacheManager::encode_repo_name(&repo), ref_))
            .collect();

        match cache_mgr.prune(&used_set) {
            Ok(_) => {
                let _ = writeln!(stdout, "✅ Pruned unused cache entries");
            }
            Err(e) => {
                let _ = writeln!(stderr, "Prune failed: {}", e);
                return 2;
            }
        }
    }

    0
}

/// Load config without expanding remote references
fn load_without_remote_expansion(path: &std::path::Path) -> Result<Config, String> {
    let data = std::fs::read_to_string(path).map_err(|e| format!("read config: {}", e))?;

    let cfg: Config = serde_yaml::from_str(&data).map_err(|e| format!("decode config: {}", e))?;

    Ok(cfg)
}

/// Collect all git remotes recursively from a config
fn collect_git_remotes_recursive(
    cfg: &Config,
    config_dir: &std::path::Path,
    cache_path: &Option<String>,
) -> Result<Vec<(String, String)>, String> {
    let mut remotes: Vec<(String, String)> = Vec::new();
    let mut visited: HashSet<std::path::PathBuf> = HashSet::new();

    collect_from_config(cfg, config_dir, cache_path, &mut remotes, &mut visited)?;

    // Remove duplicates while preserving order
    let mut seen = HashSet::new();
    let unique_remotes: Vec<(String, String)> = remotes
        .into_iter()
        .filter(|(repo, ref_)| seen.insert((repo.clone(), ref_.clone())))
        .collect();

    Ok(unique_remotes)
}

fn collect_from_config(
    cfg: &Config,
    config_dir: &std::path::Path,
    cache_path: &Option<String>,
    remotes: &mut Vec<(String, String)>,
    visited: &mut HashSet<std::path::PathBuf>,
) -> Result<(), String> {
    // Scan preconditions and rules for git remotes
    let all_rules = cfg.preconditions.iter().chain(cfg.rules.iter());

    for rule in all_rules {
        if let Some(remote_path) = &rule.remote {
            if let Some(git_remote) = parse_git_remote(remote_path) {
                remotes.push((git_remote.repo.clone(), git_remote.ref_.clone()));
            } else {
                // Regular file remote - check if it points to another config that might have git remotes
                let resolved = config_dir.join(remote_path);
                if resolved.exists() && resolved.is_file() {
                    let canonical = resolved.canonicalize().ok();
                    if let Some(path) = canonical {
                        if !visited.contains(&path) {
                            visited.insert(path.clone());
                            // Load this remote config and recurse
                            match load_without_remote_expansion(&path) {
                                Ok(remote_cfg) => {
                                    let remote_dir = path
                                        .parent()
                                        .map(|p| p.to_path_buf())
                                        .unwrap_or_else(|| config_dir.to_path_buf());
                                    collect_from_config(
                                        &remote_cfg,
                                        &remote_dir,
                                        cache_path,
                                        remotes,
                                        visited,
                                    )?;
                                }
                                Err(_) => {
                                    // Ignore configs we can't parse
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn run_schema(_args: Vec<String>, stdout: &mut dyn Write, _stderr: &mut dyn Write) -> i32 {
    let schema_json = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "checksy configuration",
  "type": "object",
  "properties": {
    "cachePath": {
      "type": "string",
      "description": "Path to cache directory for git-based remotes (defaults to .checksy-cache)",
      "default": ".checksy-cache"
    },
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
        "oneOf": [
          {
            "description": "Remote rule - only 'remote' property allowed. Supports file paths or git+https:// URLs (requires 'checksy install' first)",
            "required": ["remote"],
            "properties": {
              "remote": { 
                "type": "string", 
                "description": "Relative path to another config file, or git+URL#ref:path for git-based remotes (e.g., git+https://github.com/org/repo.git#main:.checksy.yaml)"
              }
            },
            "additionalProperties": false
          },
          {
            "description": "Inline rule - requires 'check' property",
            "required": ["check"],
            "properties": {
              "name": { "type": "string" },
              "check": { "type": "string" },
              "severity": { "type": "string", "enum": ["debug", "info", "warn", "error"], "default": "error" },
              "fix": { "type": "string" },
              "hint": { "type": "string" },
              "remote": { "type": "string" }
            },
            "additionalProperties": false
          }
        ]
      }
    },
    "rules": {
      "type": "array",
      "items": {
        "type": "object",
        "oneOf": [
          {
            "description": "Remote rule - only 'remote' property allowed. Supports file paths or git+https:// URLs (requires 'checksy install' first)",
            "required": ["remote"],
            "properties": {
              "remote": { 
                "type": "string", 
                "description": "Relative path to another config file, or git+URL#ref:path for git-based remotes (e.g., git+https://github.com/org/repo.git#main:.checksy.yaml)"
              }
            },
            "additionalProperties": false
          },
          {
            "description": "Inline rule - requires 'check' property",
            "required": ["check"],
            "properties": {
              "name": { "type": "string" },
              "check": { "type": "string" },
              "severity": { "type": "string", "enum": ["debug", "info", "warn", "error"], "default": "error" },
              "fix": { "type": "string" },
              "hint": { "type": "string" },
              "remote": { "type": "string" }
            },
            "additionalProperties": false
          }
        ]
      }
    },
    "patterns": {
      "type": "array",
      "items": { "type": "string" }
    }
  },
  "required": ["rules"],
  "additionalProperties": false
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
    let _ = writeln!(stdout, "  install    Cache git-based remote configs");
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

    let preconditions = check::filter_preconditions(&opts.config, opts.min_severity);
    for rule in preconditions {
        let result = check::run_rule(rule.clone(), workdir);
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
            check: Some(rule.fix.clone().unwrap_or_default()),
            severity: rule.severity.clone(),
            fix: None,
            hint: rule.hint.clone(),
            remote: None,
        };
        let fix_result = check::run_rule(fix_rule, workdir);
        if !fix_result.success() {
            print_rule_failure(&fix_result, stdout, stderr);
            print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
            results.push(result);
            continue;
        }

        print_rule_success(&fix_result, stdout, stderr);

        let result = check::run_rule(rule, workdir);
        print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
        results.push(result);
    }

    let rules = check::filter_rules(&opts.config, opts.min_severity);
    for rule in rules {
        let result = check::run_rule(rule.clone(), workdir);
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
            check: Some(rule.fix.clone().unwrap_or_default()),
            severity: rule.severity.clone(),
            fix: None,
            hint: rule.hint.clone(),
            remote: None,
        };
        let fix_result = check::run_rule(fix_rule, workdir);
        if !fix_result.success() {
            print_rule_failure(&fix_result, stdout, stderr);
            print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
            results.push(result);
            continue;
        }

        print_rule_success(&fix_result, stdout, stderr);

        let result = check::run_rule(rule, workdir);
        print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
        results.push(result);
    }

    let file_paths = check::expand_rule_files(workdir, &opts.config.patterns)?;
    for rel_path in file_paths {
        let result = check::run_rule_file(workdir, &rel_path);
        print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
        results.push(result);
    }

    Ok(Report {
        rules: results,
        fail_severity: opts.fail_severity,
    })
}

fn rule_display_name(rule: &Rule) -> String {
    rule.name.clone().unwrap_or_else(|| {
        rule.check
            .as_ref()
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    })
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
            check: Some("echo hi".to_string()),
            severity: None,
            fix: None,
            hint: None,
            remote: None,
        };
        assert!(rule_display_name(&rule).contains("echo hi"));

        let rule = Rule {
            name: Some("custom".to_string()),
            check: Some("echo hi".to_string()),
            severity: None,
            fix: None,
            hint: None,
            remote: None,
        };
        assert_eq!(rule_display_name(&rule), "custom");
    }
}
