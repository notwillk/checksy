use crate::cache::CacheManager;
use crate::check::{self, DiagnoseError, Options, Report, RuleResult};
use crate::config::{decode_config, load, parse_git_remote, resolve_path, resolve_remote_path};
use crate::git::GitCache;
use crate::schema::{configuration_schema, Config, Rule, Severity};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InteractionMode {
    Permitted,
    Prohibited,
    Stdin,
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
    let mut non_interactive = false;

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
            "--non-interactive" => {
                non_interactive = true;
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

    let stdin_config = resolved == "-";
    let abs_config_path = if stdin_config {
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
        let interaction = if stdin_config {
            InteractionMode::Stdin
        } else if non_interactive {
            InteractionMode::Prohibited
        } else {
            InteractionMode::Permitted
        };
        match check_with_fixes(opts, interaction, stdout, stderr) {
            Ok(r) => r,
            Err(mut e) => {
                return report_check_error(&mut e, stdout, stderr);
            }
        }
    } else {
        match check::diagnose_supervised(opts) {
            Ok(r) => r,
            Err(mut e) => {
                return report_check_error(&mut e, stdout, stderr);
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
    decode_config(&data)
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
                let path = resolve_remote_path(config_dir, cache_path.as_deref(), remote_path)?;
                if visited.insert(path.clone()) {
                    let remote_cfg = load_without_remote_expansion(&path).map_err(|error| {
                        format!(
                            "failed to load nested config '{}': {}",
                            path.display(),
                            error
                        )
                    })?;
                    let remote_dir = path
                        .parent()
                        .map(|parent| parent.to_path_buf())
                        .unwrap_or_else(|| config_dir.to_path_buf());
                    collect_from_config(&remote_cfg, &remote_dir, cache_path, remotes, visited)?;
                }
            }
        }
    }

    Ok(())
}

fn run_schema(_args: Vec<String>, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let mut output = match serde_json::to_vec_pretty(&configuration_schema()) {
        Ok(output) => output,
        Err(error) => {
            let _ = writeln!(
                stderr,
                "failed to serialize configuration schema: {}",
                error
            );
            return 2;
        }
    };
    output.push(b'\n');

    if let Err(error) = stdout.write_all(&output).and_then(|_| stdout.flush()) {
        let _ = writeln!(stderr, "failed to write configuration schema: {}", error);
        return 2;
    }

    0
}
fn print_usage(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "checksy - provision the current machine from trusted configuration"
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
    let _ = writeln!(
        stdout,
        "  check      Run checks; add --fix to provision the machine"
    );
    let _ = writeln!(
        stdout,
        "  diagnose   Run checks (deprecated, use 'check' instead)"
    );
    let _ = writeln!(stdout, "  install    Cache git-based remote configs");
    let _ = writeln!(stdout, "  init       Create a starter configuration file");
    let _ = writeln!(
        stdout,
        "  schema     Print the generated Draft 7 configuration schema"
    );
    let _ = writeln!(stdout, "  version    Print the current build version");
    let _ = writeln!(stdout, "  help       Show this message");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Check Flags:");
    let _ = writeln!(
        stdout,
        "  --fix                run configured repairs and final checks"
    );
    let _ = writeln!(
        stdout,
        "  --non-interactive    prohibit interactive-fix terminal use"
    );
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

fn report_check_error(
    error: &mut DiagnoseError,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    if let Some(execution) = error.execution() {
        let command_name = execution.command_name();
        let command_stdout = execution.stdout();
        let command_stderr = execution.stderr();
        if !command_stdout.is_empty() {
            let _ = writeln!(stderr, "{} stdout:\n{}", command_name, command_stdout);
        }
        if !command_stderr.is_empty() {
            let _ = writeln!(stderr, "{} stderr:\n{}", command_name, command_stderr);
        }
    }

    let _ = writeln!(stderr, "check failed: {}", error);

    if let Some(signal) = error
        .execution()
        .and_then(|execution| execution.interrupted_signal())
    {
        let _ = stdout.flush();
        let _ = stderr.flush();
        if let Some(execution) = error.execution_mut() {
            let _ = execution.restore_signal_handlers();
        }
        return reraise_parent_signal(signal);
    }

    2
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn reraise_parent_signal(signal: i32) -> i32 {
    // The runner has already dropped its temporary handlers before returning.
    // Invoke the default action only after caller-visible diagnostics have
    // been flushed so the invoking shell observes conventional signal status.
    // signal-hook deliberately leaves its dispatcher installed after the last
    // registration is removed. Emulate the platform default so the parent
    // observes signal termination instead of an ordinary Checksy exit code.
    let _ = signal_hook::low_level::emulate_default_handler(signal);
    2
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn reraise_parent_signal(_signal: i32) -> i32 {
    2
}

fn check_with_fixes(
    opts: Options,
    interaction: InteractionMode,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<Report, DiagnoseError> {
    opts.config
        .validate()
        .map_err(DiagnoseError::Configuration)?;

    let workdir = if opts.workdir.is_empty() {
        "."
    } else {
        &opts.workdir
    };
    let mut results = vec![];

    let preconditions = check::filter_preconditions(&opts.config, opts.min_severity);
    let rules = check::filter_rules(&opts.config, opts.min_severity);
    for rule in preconditions.into_iter().chain(rules) {
        results.push(run_rule_with_fixes(
            rule,
            workdir,
            opts.fail_severity,
            interaction,
            stdout,
            stderr,
        )?);
    }

    let file_paths = check::expand_rule_files(workdir, &opts.config.patterns)
        .map_err(DiagnoseError::Configuration)?;
    for rel_path in file_paths {
        let result = check::run_rule_file_supervised(workdir, &rel_path)?;
        print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
        results.push(result);
    }

    Ok(Report {
        rules: results,
        fail_severity: opts.fail_severity,
    })
}

fn run_rule_with_fixes(
    rule: Rule,
    workdir: &str,
    fail_severity: Severity,
    interaction: InteractionMode,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<RuleResult, DiagnoseError> {
    let original = check::run_rule_supervised(rule.clone(), workdir)?;
    if original.success() {
        print_rule_success(&original, stdout, stderr);
        return Ok(original);
    }

    if let Some(fix) = rule
        .fix
        .as_deref()
        .filter(|command| !command.trim().is_empty())
    {
        print_rule_status(&original, "⚠️ ", false, stdout, stderr);
        let fix_result = check::run_rule_supervised(repair_rule(&rule, fix), workdir)?;
        if !fix_result.success() {
            print_rule_failure(&fix_result, stdout, stderr);
            print_rule_outcome(&original, fail_severity, stdout, stderr);
            return Ok(original);
        }

        print_rule_success(&fix_result, stdout, stderr);
        let final_result = check::run_rule_supervised(rule, workdir)?;
        print_rule_outcome(&final_result, fail_severity, stdout, stderr);
        return Ok(final_result);
    }

    let Some(interactive_fix) = rule.interactive_fix.as_deref() else {
        print_rule_outcome(&original, fail_severity, stdout, stderr);
        return Ok(original);
    };

    if interaction != InteractionMode::Permitted {
        print_rule_outcome(&original, fail_severity, stdout, stderr);
        print_interactive_repair_required(&rule, interaction, stderr);
        return Ok(original);
    }

    let _ = stdout.flush();
    let _ = stderr.flush();
    let repair = repair_rule(&rule, interactive_fix);
    let fix_result = match check::run_rule_interactive_supervised(repair, workdir) {
        Ok(result) => result,
        Err(check::InteractiveExecutionError::Unavailable) => {
            print_interactive_repair_required(&rule, InteractionMode::Permitted, stderr);
            print_rule_outcome(&original, fail_severity, stdout, stderr);
            return Ok(original);
        }
        Err(check::InteractiveExecutionError::Execution(error)) => return Err(error.into()),
    };

    if !fix_result.success() {
        print_rule_failure(&fix_result, stdout, stderr);
        print_rule_outcome(&original, fail_severity, stdout, stderr);
        return Ok(original);
    }

    print_rule_success(&fix_result, stdout, stderr);
    let final_result = check::run_rule_supervised(rule, workdir)?;
    print_rule_outcome(&final_result, fail_severity, stdout, stderr);
    Ok(final_result)
}

fn repair_rule(rule: &Rule, command: &str) -> Rule {
    Rule {
        name: Some(format!("{} fix", rule_display_name(rule))),
        check: Some(command.to_string()),
        severity: rule.severity,
        fix: None,
        interactive_fix: None,
        hint: rule.hint.clone(),
        remote: None,
        timeout: rule.timeout.clone(),
    }
}

fn print_interactive_repair_required(
    rule: &Rule,
    interaction: InteractionMode,
    stderr: &mut dyn Write,
) {
    let name = rule_display_name(rule);
    let reason = match interaction {
        InteractionMode::Stdin => concat!(
            "stdin configuration is always non-interactive; save it to a local file and ",
            "rerun check --fix from a terminal"
        ),
        InteractionMode::Prohibited => concat!(
            "--non-interactive prohibits terminal use; rerun without --non-interactive ",
            "from a terminal"
        ),
        InteractionMode::Permitted => {
            "no usable controlling terminal is available; rerun check --fix from a terminal"
        }
    };
    let _ = writeln!(stderr, "{name}: interactive repair required, but {reason}");
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
    use std::io;

    fn invoke(args: &[&str]) -> (i32, String, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            args.iter().map(|arg| (*arg).to_string()).collect(),
            &mut stdout,
            &mut stderr,
        );
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    }

    struct WriteFailure;

    impl Write for WriteFailure {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "write failed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct FlushFailure {
        bytes: Vec<u8>,
    }

    impl Write for FlushFailure {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "flush failed"))
        }
    }

    #[test]
    fn help_describes_the_provisioning_cli() {
        let (code, stdout, stderr) = invoke(&["help"]);

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("provision the current machine"));
        assert!(stdout.contains("--fix"));
        for command in ["check", "diagnose", "install", "init", "schema", "version"] {
            assert!(stdout.contains(command), "help omitted {command}");
        }
        assert!(stdout.contains("--non-interactive"));
        assert!(stdout.contains("interactive-fix"));
        assert!(!stdout.contains("apply"));
    }

    #[test]
    fn schema_command_matches_direct_generation_and_is_deterministic() {
        let (first_code, first_stdout, first_stderr) = invoke(&["schema"]);
        let (second_code, second_stdout, second_stderr) = invoke(&["schema"]);

        assert_eq!(first_code, 0);
        assert_eq!(second_code, 0);
        assert!(first_stderr.is_empty());
        assert!(second_stderr.is_empty());
        assert_eq!(first_stdout, second_stdout);

        let mut expected = serde_json::to_vec_pretty(&configuration_schema()).unwrap();
        expected.push(b'\n');
        assert_eq!(first_stdout.as_bytes(), expected);
        assert!(first_stdout.ends_with('\n'));
        assert!(!first_stdout.ends_with("\n\n"));
    }

    #[test]
    fn schema_command_reports_write_and_flush_failures() {
        for (mut stdout, expected_message) in [
            (Box::new(WriteFailure) as Box<dyn Write>, "write failed"),
            (
                Box::new(FlushFailure::default()) as Box<dyn Write>,
                "flush failed",
            ),
        ] {
            let mut stderr = Vec::new();
            let code = run(vec!["schema".to_string()], stdout.as_mut(), &mut stderr);

            assert_eq!(code, 2);
            let stderr = String::from_utf8(stderr).unwrap();
            assert!(stderr.contains("failed to write configuration schema"));
            assert!(stderr.contains(expected_message));
        }
    }

    #[test]
    fn stable_exit_classes_cover_usage_operations_and_compliance() {
        let (code, _, _) = invoke(&[]);
        assert_eq!(code, 1);

        let (code, _, _) = invoke(&["unknown-command"]);
        assert_eq!(code, 2);

        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.yaml");
        let missing = missing.to_string_lossy().into_owned();
        let (code, _, _) = invoke(&["--config", &missing, "check"]);
        assert_eq!(code, 2);
        let (code, _, _) = invoke(&["--config", &missing, "check", "--no-fail"]);
        assert_eq!(code, 2);

        let passing = directory.path().join("passing.yaml");
        std::fs::write(
            &passing,
            "rules:\n  - name: passing\n    check: 'true'\n    severity: error\n",
        )
        .unwrap();
        let passing = passing.to_string_lossy().into_owned();
        let (code, _, _) = invoke(&["--config", &passing, "check"]);
        assert_eq!(code, 0);

        let failing = directory.path().join("failing.yaml");
        std::fs::write(
            &failing,
            "rules:\n  - name: failing\n    check: 'false'\n    severity: error\n",
        )
        .unwrap();
        let failing = failing.to_string_lossy().into_owned();
        let (code, _, _) = invoke(&["--config", &failing, "check"]);
        assert_eq!(code, 3);

        let (code, _, _) = invoke(&["--config", &failing, "check", "--no-fail"]);
        assert_eq!(code, 0);
    }

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
    fn non_interactive_is_a_bare_check_flag_and_does_not_imply_fix() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("passing.yaml");
        std::fs::write(
            &config,
            concat!(
                "rules:\n",
                "  - check: 'true'\n",
                "    interactive-fix: 'exit 99'\n"
            ),
        )
        .unwrap();
        let config = config.to_string_lossy();

        let (code, stdout, stderr) =
            invoke(&["--config", config.as_ref(), "check", "--non-interactive"]);
        assert_eq!(code, 0);
        assert!(stdout.contains("All rules validated"));
        assert!(stderr.is_empty());

        let (code, _, stderr) = invoke(&[
            "--config",
            config.as_ref(),
            "check",
            "--non-interactive=true",
        ]);
        assert_eq!(code, 2);
        assert!(stderr.contains("Unknown flag"));
    }

    #[test]
    fn test_rule_display_name() {
        let rule = Rule {
            name: None,
            check: Some("echo hi".to_string()),
            severity: None,
            fix: None,
            interactive_fix: None,
            hint: None,
            remote: None,
            timeout: None,
        };
        assert!(rule_display_name(&rule).contains("echo hi"));

        let rule = Rule {
            name: Some("custom".to_string()),
            check: Some("echo hi".to_string()),
            severity: None,
            fix: None,
            interactive_fix: None,
            hint: None,
            remote: None,
            timeout: None,
        };
        assert_eq!(rule_display_name(&rule), "custom");
    }
}
