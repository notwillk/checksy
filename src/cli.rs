use crate::cache::CacheManager;
use crate::check::{self, Report, ResolvedOptions, RuleFailureKind, RuleResult};
use crate::config::{
    load_resolved_with_diagnostics, prepare_resolved_root, resolve_path, ConfigDiagnostic,
    PreparedRoot,
};
use crate::git::GitCache;
use crate::resolved::{GitDependency, ResolvedLoad, ResolvedRule, ResolverMode};
use crate::schema::{configuration_schema, Rule, Severity};
use crate::state_lock::{LockError, StateDirectoryLock};
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

    // File-backed fix mode is one complete mutation scope. Freeze the selected
    // root before locking so every discovery pass uses the same cache root,
    // then keep the guard alive through acquisition, checks, fixes, and the
    // final report. Stdin has no stable state/cache root and cannot expand
    // remotes, so it retains its legacy unlocked behavior.
    let _mutation_lock: Option<StateDirectoryLock>;
    let loaded = if apply_fixes && abs_config_path != "-" {
        let mut prepared = match prepare_resolved_root(&abs_config_path) {
            Ok(prepared) => prepared,
            Err(error) => {
                writeln!(
                    stderr,
                    "failed to load config '{}': {}",
                    abs_config_path, error
                )
                .ok();
                return 2;
            }
        };
        _mutation_lock = match acquire_mutation_lock(&mut prepared, stderr) {
            Ok(lock) => Some(lock),
            Err(exit) => return exit,
        };
        match load_resolved_with_fix(&prepared, stdout) {
            Ok(loaded) => loaded,
            Err(error) => {
                writeln!(
                    stderr,
                    "failed to load config '{}': {}",
                    abs_config_path, error
                )
                .ok();
                return 2;
            }
        }
    } else {
        _mutation_lock = None;
        match load_resolved_with_diagnostics(&abs_config_path) {
            Ok(loaded) => loaded,
            Err(error) => {
                writeln!(
                    stderr,
                    "failed to load config '{}': {}",
                    abs_config_path, error
                )
                .ok();
                return 2;
            }
        }
    };
    print_config_diagnostics(&loaded.diagnostics, stderr);
    let definition = loaded.definition;

    let check_severity = if let Some(ref s) = check_severity {
        match parse_severity(s) {
            Ok(sev) => sev,
            Err(e) => {
                writeln!(stderr, "{}", e).ok();
                return 2;
            }
        }
    } else if let Some(s) = definition.check_severity {
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
    } else if let Some(s) = definition.fail_severity {
        s
    } else {
        Severity::Error
    };

    let min_severity = check::min_severity(check_severity, fail_severity);

    let opts = ResolvedOptions {
        definition,
        min_severity,
        fail_severity,
    };

    let report = if apply_fixes {
        match check_with_fixes(opts, stdout, stderr) {
            Ok(r) => r,
            Err(e) => {
                writeln!(stderr, "{}", e).ok();
                return 2;
            }
        }
    } else {
        match check::diagnose_resolved(opts) {
            Ok(r) => r,
            Err(e) => {
                writeln!(stderr, "check failed: {}", e).ok();
                return 2;
            }
        }
    };

    if let Some(failure) = report.first_operational_failure() {
        if !apply_fixes {
            print_report_results(&report, stdout, stderr);
        }
        print_operational_failure(failure, stderr);
        return 2;
    }

    if !apply_fixes {
        print_report_results(&report, stdout, stderr);
    }

    summarize_report(&report, no_fail, stdout)
}

/// Resolve a complete definition, optionally materializing every missing Git
/// dependency before retrying. Discovery is repeated because a newly cloned
/// parent can reveal more nested Git references.
fn load_resolved_with_fix(
    prepared: &PreparedRoot,
    stdout: &mut dyn Write,
) -> Result<ResolvedLoad, String> {
    let mut announced = false;
    loop {
        let discovered = prepared.resolve(ResolverMode::CacheMissing)?;
        let missing: Vec<GitDependency> = discovered
            .git_dependencies
            .into_iter()
            .filter(|dependency| !dependency.cached)
            .collect();

        if missing.is_empty() {
            return prepared.resolve(ResolverMode::CachedOnly);
        }

        if !announced {
            writeln!(stdout, "🔧 Caching missing Git remotes...").ok();
            announced = true;
        }

        for dependency in &missing {
            let remote = &dependency.remote;
            let cache = CacheManager::from_root(dependency.cache_root.clone());
            let _ = write!(stdout, "  {}#{} ", remote.repo, remote.ref_);
            let destination = match cache.prepare_ref_cache_path(&remote.repo, &remote.ref_) {
                Ok(destination) => destination,
                Err(error) => {
                    let _ = writeln!(stdout, "✗");
                    return Err(error);
                }
            };
            match GitCache::shallow_clone(&remote.repo, &remote.ref_, &destination) {
                Ok(()) => {
                    let _ = writeln!(stdout, "✓");
                }
                Err(error) => {
                    let _ = writeln!(stdout, "✗");
                    return Err(format!(
                        "failed to cache {}#{}: {}",
                        remote.repo, remote.ref_, error
                    ));
                }
            }
        }

        writeln!(stdout, "✅ Git remotes cached, retrying...").ok();
    }
}

fn acquire_mutation_lock(
    prepared: &mut PreparedRoot,
    stderr: &mut dyn Write,
) -> Result<StateDirectoryLock, i32> {
    let selected_root = prepared.cache_root().to_path_buf();
    match StateDirectoryLock::acquire(&selected_root) {
        Ok(lock) => {
            prepared.use_canonical_cache_root(lock.canonical_root());
            Ok(lock)
        }
        Err(LockError::Held) => {
            writeln!(
                stderr,
                "lock-held: mutation lock is already held for '{}'",
                selected_root.display()
            )
            .ok();
            Err(4)
        }
        Err(LockError::State(error)) => {
            writeln!(stderr, "state-failed: {}", error).ok();
            Err(2)
        }
        Err(LockError::UnsupportedPlatform) => {
            writeln!(
                stderr,
                "unsupported-platform: state directory locking is supported only on Linux and macOS"
            )
            .ok();
            Err(2)
        }
    }
}

fn print_config_diagnostics(diagnostics: &[ConfigDiagnostic], stderr: &mut dyn Write) {
    for diagnostic in diagnostics {
        let _ = writeln!(stderr, "{}", diagnostic);
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

    let config_path = abs_config_path.to_string_lossy().into_owned();
    let mut prepared = match prepare_resolved_root(&config_path) {
        Ok(prepared) => prepared,
        Err(error) => {
            writeln!(stderr, "failed to load config '{}': {}", config_path, error).ok();
            return 2;
        }
    };
    let _mutation_lock = match acquire_mutation_lock(&mut prepared, stderr) {
        Ok(lock) => lock,
        Err(exit) => return exit,
    };
    let mut processed: HashSet<(std::path::PathBuf, String, String)> = HashSet::new();
    let mut expanded = HashSet::new();
    let mut announced = false;

    loop {
        let discovered = match prepared.resolve_for_install(&expanded) {
            Ok(discovered) => discovered,
            Err(error) => {
                writeln!(stderr, "failed to collect git remotes: {}", error).ok();
                return 2;
            }
        };

        let pending: Vec<GitDependency> = discovered
            .git_dependencies
            .iter()
            .filter(|dependency| {
                !processed.contains(&(
                    dependency.cache_root.clone(),
                    dependency.remote.repo.clone(),
                    dependency.remote.ref_.clone(),
                ))
            })
            .cloned()
            .collect();

        if pending.is_empty() {
            break;
        }

        if !announced {
            let _ = writeln!(stdout, "📦 Caching Git remotes...");
            announced = true;
        }

        for dependency in pending {
            if let Err(error) = refresh_git_dependency(&dependency, stdout) {
                let _ = writeln!(stderr, "{}", error);
                return 2;
            }
            processed.insert((
                dependency.cache_root.clone(),
                dependency.remote.repo.clone(),
                dependency.remote.ref_.clone(),
            ));
            expanded.insert((dependency.remote.repo, dependency.remote.ref_));
        }
    }

    // The frontier walk deliberately defers unrefreshed Git parsing. Validate
    // the complete fresh graph once more before reporting success or pruning.
    let final_load = match prepared.resolve(ResolverMode::CachedOnly) {
        Ok(loaded) => loaded,
        Err(error) => {
            writeln!(stderr, "failed to load refreshed config: {}", error).ok();
            return 2;
        }
    };
    print_config_diagnostics(&final_load.diagnostics, stderr);

    if final_load.git_dependencies.is_empty() {
        writeln!(stdout, "No git remotes to cache").ok();
        return 0;
    }

    let _ = writeln!(stdout, "✅ All remotes cached");

    if prune {
        let mut roots: std::collections::HashMap<std::path::PathBuf, HashSet<(String, String)>> =
            std::collections::HashMap::new();
        for dependency in &final_load.git_dependencies {
            roots
                .entry(dependency.cache_root.clone())
                .or_default()
                .insert((
                    CacheManager::encode_repo_name(&dependency.remote.repo),
                    CacheManager::encode_ref_name(&dependency.remote.ref_),
                ));
        }

        for (root, used) in roots {
            if let Err(error) = CacheManager::from_root(root).prune(&used) {
                let _ = writeln!(stderr, "Prune failed: {}", error);
                return 2;
            }
        }
        let _ = writeln!(stdout, "✅ Pruned unused cache entries");
    }

    0
}

fn refresh_git_dependency(
    dependency: &GitDependency,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let remote = &dependency.remote;
    let cache = CacheManager::from_root(dependency.cache_root.clone());
    let _ = write!(stdout, "  {}#{} ", remote.repo, remote.ref_);
    let cache_path = match cache.prepare_ref_cache_path(&remote.repo, &remote.ref_) {
        Ok(cache_path) => cache_path,
        Err(error) => {
            let _ = writeln!(stdout, "✗");
            return Err(error);
        }
    };

    if dependency.cached {
        let local_sha = match GitCache::get_local_sha(&cache_path) {
            Ok(sha) => sha,
            Err(error) => {
                let _ = writeln!(stdout, "✗");
                return Err(format!(
                    "Failed to read local cache for {}#{}: {}",
                    remote.repo, remote.ref_, error
                ));
            }
        };
        let remote_sha = match GitCache::get_remote_sha(&remote.repo, &remote.ref_) {
            Ok(sha) => sha,
            Err(error) => {
                let _ = writeln!(stdout, "✗");
                return Err(format!(
                    "Failed to check remote for {}#{}: {}",
                    remote.repo, remote.ref_, error
                ));
            }
        };

        if local_sha == remote_sha {
            let _ = writeln!(stdout, "✓ (already cached)");
            return Ok(());
        }

        let _ = writeln!(stdout, "↑ updating...");
        let cache_path = cache.confined_ref_cache_path(&remote.repo, &remote.ref_)?;
        if let Err(error) = std::fs::remove_dir_all(&cache_path) {
            let _ = writeln!(stdout, "✗");
            return Err(format!(
                "Failed to remove old cache for {}#{}: {}",
                remote.repo, remote.ref_, error
            ));
        }
    }

    let cache_path = cache.prepare_ref_cache_path(&remote.repo, &remote.ref_)?;
    if let Err(error) = GitCache::shallow_clone(&remote.repo, &remote.ref_, &cache_path) {
        let _ = writeln!(stdout, "✗");
        return Err(format!(
            "Failed to cache {}#{}: {}",
            remote.repo, remote.ref_, error
        ));
    }
    let _ = writeln!(stdout, "✓");
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
        if result.operational_failure() {
            print_rule_failure(result, stdout, stderr);
        } else {
            print_rule_outcome(result, fail_severity, stdout, stderr);
        }
    }
}

fn print_operational_failure(result: &RuleResult, stderr: &mut dyn Write) {
    let message = result
        .err
        .as_deref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "command runner failed".to_string());
    if result.failure_kind() == Some(RuleFailureKind::Timeout) {
        let _ = writeln!(stderr, "operation-timeout: {}: {}", result.name(), message);
    } else {
        let _ = writeln!(stderr, "check failed: {}: {}", result.name(), message);
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

#[derive(Debug)]
enum CheckWithFixesError {
    Planning(String),
    Operational {
        name: String,
        kind: RuleFailureKind,
        message: String,
    },
}

impl std::fmt::Display for CheckWithFixesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planning(message) => write!(formatter, "check failed: {message}"),
            Self::Operational {
                name,
                kind: RuleFailureKind::Timeout,
                message,
            } => write!(formatter, "operation-timeout: {name}: {message}"),
            Self::Operational {
                name,
                kind: _,
                message,
            } => write!(formatter, "check failed: {name}: {message}"),
        }
    }
}

fn operational_fix_error(result: &RuleResult) -> CheckWithFixesError {
    CheckWithFixesError::Operational {
        name: result.name(),
        kind: result
            .failure_kind()
            .unwrap_or(RuleFailureKind::Supervision),
        message: result
            .err
            .as_deref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "command runner failed".to_string()),
    }
}

fn check_with_fixes(
    opts: ResolvedOptions,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<Report, CheckWithFixesError> {
    check::validate_resolved_timeouts(
        &opts.definition,
        crate::process_runner::DEFAULT_COMMAND_TIMEOUT,
    )
    .map_err(CheckWithFixesError::Planning)?;
    // Resolve and confine every pattern match before a check or fix can mutate
    // the host. Pattern-only definitions are valid and must reach execution.
    let rule_files = check::expand_resolved_rule_files(&opts.definition.pattern_groups)
        .map_err(CheckWithFixesError::Planning)?;
    let mut results = vec![];

    let preconditions =
        check::filter_resolved_rules(&opts.definition.preconditions, opts.min_severity);
    for resolved_rule in preconditions {
        let rule = resolved_rule.rule.clone();
        let result = check::run_resolved_rule(resolved_rule.clone());
        if result.operational_failure() {
            print_rule_failure(&result, stdout, stderr);
            return Err(operational_fix_error(&result));
        }
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

        let fix_rule = ResolvedRule {
            rule: Rule {
                name: Some(format!("{} fix", rule_display_name(&rule))),
                check: Some(rule.fix.clone().unwrap_or_default()),
                severity: rule.severity,
                timeout: rule.timeout.clone(),
                fix: None,
                hint: rule.hint.clone(),
                remote: None,
            },
            origin: resolved_rule.origin.clone(),
        };
        let fix_result = check::run_resolved_rule(fix_rule);
        if fix_result.operational_failure() {
            print_rule_failure(&fix_result, stdout, stderr);
            return Err(operational_fix_error(&fix_result));
        }
        if !fix_result.success() {
            print_rule_failure(&fix_result, stdout, stderr);
            print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
            results.push(result);
            continue;
        }

        print_rule_success(&fix_result, stdout, stderr);

        let result = check::run_resolved_rule(resolved_rule);
        if result.operational_failure() {
            print_rule_failure(&result, stdout, stderr);
            return Err(operational_fix_error(&result));
        }
        print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
        results.push(result);
    }

    let rules = check::filter_resolved_rules(&opts.definition.rules, opts.min_severity);
    for resolved_rule in rules {
        let rule = resolved_rule.rule.clone();
        let result = check::run_resolved_rule(resolved_rule.clone());
        if result.operational_failure() {
            print_rule_failure(&result, stdout, stderr);
            return Err(operational_fix_error(&result));
        }
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

        let fix_rule = ResolvedRule {
            rule: Rule {
                name: Some(format!("{} fix", rule_display_name(&rule))),
                check: Some(rule.fix.clone().unwrap_or_default()),
                severity: rule.severity,
                timeout: rule.timeout.clone(),
                fix: None,
                hint: rule.hint.clone(),
                remote: None,
            },
            origin: resolved_rule.origin.clone(),
        };
        let fix_result = check::run_resolved_rule(fix_rule);
        if fix_result.operational_failure() {
            print_rule_failure(&fix_result, stdout, stderr);
            return Err(operational_fix_error(&fix_result));
        }
        if !fix_result.success() {
            print_rule_failure(&fix_result, stdout, stderr);
            print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
            results.push(result);
            continue;
        }

        print_rule_success(&fix_result, stdout, stderr);

        let result = check::run_resolved_rule(resolved_rule);
        if result.operational_failure() {
            print_rule_failure(&result, stdout, stderr);
            return Err(operational_fix_error(&result));
        }
        print_rule_outcome(&result, opts.fail_severity, stdout, stderr);
        results.push(result);
    }

    for rule_file in rule_files {
        let result = check::run_resolved_rule_file(rule_file);
        if result.operational_failure() {
            print_rule_failure(&result, stdout, stderr);
            return Err(operational_fix_error(&result));
        }
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

    struct WriteFailure;

    impl Write for WriteFailure {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "injected write failure",
            ))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct FlushFailure(Vec<u8>);

    impl Write for FlushFailure {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "injected flush failure",
            ))
        }
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
    fn test_rule_display_name() {
        let rule = Rule {
            name: None,
            check: Some("echo hi".to_string()),
            severity: None,
            timeout: None,
            fix: None,
            hint: None,
            remote: None,
        };
        assert!(rule_display_name(&rule).contains("echo hi"));

        let rule = Rule {
            name: Some("custom".to_string()),
            check: Some("echo hi".to_string()),
            severity: None,
            timeout: None,
            fix: None,
            hint: None,
            remote: None,
        };
        assert_eq!(rule_display_name(&rule), "custom");
    }

    #[test]
    fn test_schema_command_matches_generated_schema_deterministically() {
        let mut first_stdout = Vec::new();
        let mut first_stderr = Vec::new();
        let first_code = run(
            vec!["schema".to_string()],
            &mut first_stdout,
            &mut first_stderr,
        );

        let mut second_stdout = Vec::new();
        let mut second_stderr = Vec::new();
        let second_code = run(
            vec!["schema".to_string()],
            &mut second_stdout,
            &mut second_stderr,
        );

        assert_eq!(first_code, 0);
        assert_eq!(second_code, 0);
        assert!(first_stderr.is_empty());
        assert!(second_stderr.is_empty());
        assert_eq!(first_stdout, second_stdout);

        let mut expected = serde_json::to_vec_pretty(&configuration_schema()).unwrap();
        expected.push(b'\n');
        assert_eq!(first_stdout, expected);
        assert!(first_stdout.ends_with(b"\n"));
        assert!(!first_stdout.ends_with(b"\n\n"));

        let parsed: serde_json::Value = serde_json::from_slice(&first_stdout).unwrap();
        assert_eq!(parsed["$schema"], "http://json-schema.org/draft-07/schema#");
    }

    #[test]
    fn test_schema_command_reports_write_failure() {
        let mut stdout = WriteFailure;
        let mut stderr = Vec::new();

        let code = run(vec!["schema".to_string()], &mut stdout, &mut stderr);

        assert_eq!(code, 2);
        assert!(String::from_utf8(stderr)
            .unwrap()
            .contains("failed to write configuration schema"));
    }

    #[test]
    fn test_schema_command_reports_flush_failure() {
        let mut stdout = FlushFailure::default();
        let mut stderr = Vec::new();

        let code = run(vec!["schema".to_string()], &mut stdout, &mut stderr);

        assert_eq!(code, 2);
        assert!(!stdout.0.is_empty());
        assert!(String::from_utf8(stderr)
            .unwrap()
            .contains("failed to write configuration schema"));
    }

    #[test]
    fn test_mixed_case_config_severities_emit_location_aware_warnings() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join("strict-config")
            .join("valid")
            .join("mixed-case-severity.yaml");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", fixture.display()),
                "check".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 0);
        let stderr = String::from_utf8(stderr).unwrap();
        let warnings: Vec<_> = stderr
            .lines()
            .filter(|line| line.starts_with("warning:"))
            .collect();
        assert_eq!(warnings.len(), 2, "unexpected stderr: {stderr:?}");
        assert!(stderr.contains("checkSeverity"));
        assert!(stderr.contains("use 'debug'"));
        assert!(stderr.contains("failSeverity"));
        assert!(stderr.contains("use 'error'"));
    }

    #[test]
    fn test_install_reports_config_severity_deprecations() {
        let source_fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join("strict-config")
            .join("valid")
            .join("mixed-case-severity.yaml");
        let temp = tempfile::TempDir::new().unwrap();
        let fixture = temp.path().join("mixed-case-severity.yaml");
        std::fs::copy(source_fixture, &fixture).unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", fixture.display()),
                "install".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 0);
        assert!(String::from_utf8(stdout)
            .unwrap()
            .contains("No git remotes to cache"));
        let stderr = String::from_utf8(stderr).unwrap();
        assert_eq!(
            stderr
                .lines()
                .filter(|line| line.starts_with("warning:"))
                .count(),
            2
        );
    }

    #[test]
    fn test_lowercase_config_severity_aliases_do_not_warn() {
        let dir = tempfile::TempDir::new().unwrap();
        let fixture = dir.path().join("config.yaml");
        std::fs::write(
            &fixture,
            "checkSeverity: warn\nfailSeverity: warning\nrules: []\n",
        )
        .unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", fixture.display()),
                "check".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 0);
        assert!(
            stderr.is_empty(),
            "unexpected stderr: {:?}",
            String::from_utf8(stderr)
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_command_timeout_is_operational_and_no_fail_cannot_mask_it() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = temp.path().join("config.yaml");
        std::fs::write(
            &config,
            concat!(
                "rules:\n",
                "  - name: bounded command\n",
                "    check: printf before-timeout; exec sleep 10\n",
                "    timeout: 250ms\n"
            ),
        )
        .unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", config.display()),
                "check".to_string(),
                "--no-fail".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 2);
        assert!(String::from_utf8(stdout)
            .unwrap()
            .contains("❌ bounded command"));
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("before-timeout"), "{stderr:?}");
        assert!(stderr.contains("operation-timeout:"), "{stderr:?}");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_fix_timeout_is_operational_and_stops_before_recheck() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = temp.path().join("config.yaml");
        let check_log = temp.path().join("checks.log");
        let fix_marker = temp.path().join("fix-started");
        std::fs::write(
            &config,
            format!(
                concat!(
                    "cachePath: cache\n",
                    "rules:\n",
                    "  - name: bounded fix\n",
                    "    check: echo check >> {0}; false\n",
                    "    fix: printf fix-before-timeout; touch {1}; exec sleep 10\n",
                    "    timeout: 250ms\n"
                ),
                check_log.display(),
                fix_marker.display()
            ),
        )
        .unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", config.display()),
                "check".to_string(),
                "--fix".to_string(),
                "--no-fail".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 2);
        assert!(fix_marker.exists(), "the fix must have started");
        assert_eq!(std::fs::read_to_string(&check_log).unwrap(), "check\n");
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("fix-before-timeout"), "{stderr:?}");
        assert!(stderr.contains("operation-timeout:"), "{stderr:?}");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_signal_termination_is_operational_and_fix_is_not_attempted() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = temp.path().join("config.yaml");
        let fix_marker = temp.path().join("fix-must-not-run");
        std::fs::write(
            &config,
            format!(
                concat!(
                    "cachePath: cache\n",
                    "rules:\n",
                    "  - name: terminated command\n",
                    "    severity: warn\n",
                    "    check: kill -TERM $$\n",
                    "    fix: touch {}\n"
                ),
                fix_marker.display()
            ),
        )
        .unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", config.display()),
                "check".to_string(),
                "--fix".to_string(),
                "--no-fail".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 2);
        assert!(!fix_marker.exists());
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(
            stderr.contains("command terminated by signal"),
            "{stderr:?}"
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_rule_timeout_is_reapplied_to_the_post_fix_recheck() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = temp.path().join("config.yaml");
        let check_log = temp.path().join("checks.log");
        let fixed_marker = temp.path().join("fixed");
        std::fs::write(
            &config,
            format!(
                concat!(
                    "cachePath: cache\n",
                    "rules:\n",
                    "  - name: bounded recheck\n",
                    "    check: echo check >> {0}; if test -f {1}; then printf recheck-before-timeout; exec sleep 10; else false; fi\n",
                    "    fix: touch {1}\n",
                    "    timeout: 250ms\n"
                ),
                check_log.display(),
                fixed_marker.display()
            ),
        )
        .unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", config.display()),
                "check".to_string(),
                "--fix".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 2);
        assert_eq!(
            std::fs::read_to_string(&check_log).unwrap(),
            "check\ncheck\n"
        );
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("recheck-before-timeout"), "{stderr:?}");
        assert!(stderr.contains("operation-timeout:"), "{stderr:?}");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_rule_timeout_cannot_raise_global_limit_and_is_preflighted() {
        let temp = tempfile::TempDir::new().unwrap();
        let marker = temp.path().join("must-not-run");
        let config = temp.path().join("config.yaml");
        std::fs::write(
            &config,
            format!(
                concat!(
                    "rules:\n",
                    "  - check: touch {0}\n",
                    "  - check: 'true'\n",
                    "    timeout: 16m\n"
                ),
                marker.display()
            ),
        )
        .unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", config.display()),
                "check".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 2);
        assert!(!marker.exists());
        assert!(stdout.is_empty(), "unexpected stdout: {stdout:?}");
        assert!(String::from_utf8(stderr)
            .unwrap()
            .contains("exceeds the effective command timeout"));
    }

    #[test]
    fn test_origin_regression_fixture_uses_defining_directories_without_mutation() {
        fn snapshot_tree(root: &std::path::Path) -> Vec<(std::path::PathBuf, String, Vec<u8>)> {
            fn visit(
                root: &std::path::Path,
                directory: &std::path::Path,
                snapshot: &mut Vec<(std::path::PathBuf, String, Vec<u8>)>,
            ) {
                let mut entries = std::fs::read_dir(directory)
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                entries.sort_by_key(|entry| entry.file_name());

                for entry in entries {
                    let path = entry.path();
                    let relative = path.strip_prefix(root).unwrap().to_path_buf();
                    let file_type = entry.file_type().unwrap();
                    if file_type.is_dir() {
                        snapshot.push((relative, "directory".to_string(), Vec::new()));
                        visit(root, &path, snapshot);
                    } else if file_type.is_file() {
                        snapshot.push((relative, "file".to_string(), std::fs::read(path).unwrap()));
                    } else if file_type.is_symlink() {
                        snapshot.push((
                            relative,
                            "symlink".to_string(),
                            std::fs::read_link(path)
                                .unwrap()
                                .to_string_lossy()
                                .into_owned()
                                .into_bytes(),
                        ));
                    } else {
                        panic!("unsupported fixture entry: {}", path.display());
                    }
                }
            }

            let mut snapshot = Vec::new();
            visit(root, root, &mut snapshot);
            snapshot
        }

        let fixture_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join("origin-regression")
            .canonicalize()
            .unwrap();
        let fixture = fixture_root.join(".checksy.yaml");
        let before = snapshot_tree(&fixture_root);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", fixture.display()),
                "check".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        let after = snapshot_tree(&fixture_root);
        assert_eq!(after, before, "origin regression fixture was mutated");
        assert_eq!(
            code,
            0,
            "stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        assert!(
            stderr.is_empty(),
            "unexpected stderr: {:?}",
            String::from_utf8_lossy(&stderr)
        );

        let stdout = String::from_utf8(stdout).unwrap();
        assert_eq!(
            stdout,
            concat!(
                "✅ root inline origin\n",
                "✅ nested inline origin\n",
                "✅ scripts/root-origin.sh\n",
                "✅ scripts/nested-origin.sh\n",
                "😎 All rules validated\n"
            )
        );
        assert!(!stdout.contains("scripts/excluded.sh"));
        assert!(!stdout.contains("ROOT_EXCLUDED_PATTERN_EXECUTED"));
        assert!(!stdout.contains("NESTED_EXCLUDED_PATTERN_EXECUTED"));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_install_lock_contention_returns_exit_four_before_progress() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = temp.path().join("config.yaml");
        std::fs::write(&config, "cachePath: cache\nrules: []\n").unwrap();
        let cache_root = temp.path().join("cache");
        let _held = StateDirectoryLock::acquire(&cache_root).unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", config.display()),
                "install".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 4);
        assert!(stdout.is_empty(), "unexpected stdout: {stdout:?}");
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.starts_with("lock-held:"), "{stderr:?}");
        assert!(stderr.contains(&cache_root.display().to_string()));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_check_fix_lock_contention_prevents_fix_and_no_fail_cannot_mask_it() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = temp.path().join("config.yaml");
        std::fs::write(
            &config,
            concat!(
                "cachePath: cache\n",
                "rules:\n",
                "  - name: must not fix\n",
                "    check: test -f fixed\n",
                "    fix: touch fixed\n"
            ),
        )
        .unwrap();
        let cache_root = temp.path().join("cache");
        let _held = StateDirectoryLock::acquire(&cache_root).unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", config.display()),
                "check".to_string(),
                "--fix".to_string(),
                "--no-fail".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 4);
        assert!(stdout.is_empty(), "unexpected stdout: {stdout:?}");
        assert!(!temp.path().join("fixed").exists());
        assert!(String::from_utf8(stderr).unwrap().starts_with("lock-held:"));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_deprecated_diagnose_fix_uses_the_same_mutation_lock() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = temp.path().join("config.yaml");
        std::fs::write(
            &config,
            "cachePath: cache\nrules:\n  - check: test -f fixed\n    fix: touch fixed\n",
        )
        .unwrap();
        let _held = StateDirectoryLock::acquire(&temp.path().join("cache")).unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", config.display()),
                "diagnose".to_string(),
                "--fix".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 4);
        assert!(stdout.is_empty(), "unexpected stdout: {stdout:?}");
        assert!(!temp.path().join("fixed").exists());
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("is deprecated"), "{stderr:?}");
        assert!(stderr.contains("lock-held:"), "{stderr:?}");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_lock_integrity_failure_is_operational_not_contention() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = temp.path().join("config.yaml");
        std::fs::write(&config, "cachePath: cache\nrules: []\n").unwrap();
        let cache_root = temp.path().join("cache");
        std::fs::create_dir_all(cache_root.join("lock")).unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", config.display()),
                "install".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 2);
        assert!(stdout.is_empty(), "unexpected stdout: {stdout:?}");
        assert!(String::from_utf8(stderr)
            .unwrap()
            .starts_with("state-failed:"));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_ordinary_check_remains_lock_free() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = temp.path().join("config.yaml");
        std::fs::write(
            &config,
            "cachePath: cache\nrules:\n  - name: ordinary check\n    check: 'true'\n",
        )
        .unwrap();
        let _held = StateDirectoryLock::acquire(&temp.path().join("cache")).unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", config.display()),
                "check".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 0);
        assert!(stderr.is_empty(), "unexpected stderr: {stderr:?}");
        assert_eq!(
            String::from_utf8(stdout).unwrap(),
            "✅ ordinary check\n😎 All rules validated\n"
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_canonical_cache_aliases_share_one_lock() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::TempDir::new().unwrap();
        let real_cache = temp.path().join("real-cache");
        let alias_cache = temp.path().join("cache-alias");
        std::fs::create_dir(&real_cache).unwrap();
        symlink(&real_cache, &alias_cache).unwrap();
        let _held = StateDirectoryLock::acquire(&real_cache).unwrap();
        let config = temp.path().join("config.yaml");
        std::fs::write(&config, "cachePath: cache-alias\nrules: []\n").unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(
            vec![
                format!("--config={}", config.display()),
                "install".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 4);
        assert!(stdout.is_empty(), "unexpected stdout: {stdout:?}");
        assert!(String::from_utf8(stderr).unwrap().starts_with("lock-held:"));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_mutation_lock_rebinds_prepared_root_before_alias_retarget() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::TempDir::new().unwrap();
        let first_cache = temp.path().join("first-cache");
        let second_cache = temp.path().join("second-cache");
        let alias_cache = temp.path().join("cache-alias");
        std::fs::create_dir(&first_cache).unwrap();
        std::fs::create_dir(&second_cache).unwrap();
        symlink(&first_cache, &alias_cache).unwrap();

        let config = temp.path().join("config.yaml");
        std::fs::write(&config, "cachePath: cache-alias\nrules: []\n").unwrap();
        let mut prepared = prepare_resolved_root(config.to_str().unwrap()).unwrap();
        let mut stderr = Vec::new();
        let _lock = acquire_mutation_lock(&mut prepared, &mut stderr).unwrap();
        assert!(stderr.is_empty());
        assert_eq!(prepared.cache_root(), first_cache.canonicalize().unwrap());

        std::fs::remove_file(&alias_cache).unwrap();
        symlink(&second_cache, &alias_cache).unwrap();
        assert_eq!(prepared.cache_root(), first_cache.canonicalize().unwrap());
    }

    #[test]
    fn test_cli_uses_nested_origins_for_checks_fixes_and_patterns() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();
        let child = root.join("child");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(root.join("root-marker"), "root\n").unwrap();
        std::fs::write(child.join("child-marker"), "child\n").unwrap();
        std::fs::write(
            root.join("root.sh"),
            "test -f root-marker && touch root-pattern-ran\n",
        )
        .unwrap();
        std::fs::write(
            child.join("run.sh"),
            "test -f child-marker && test -f repaired && touch child-pattern-ran\n",
        )
        .unwrap();
        std::fs::write(child.join("skip.sh"), "touch skipped-pattern-ran\nexit 1\n").unwrap();
        std::fs::write(
            child.join("child.yaml"),
            concat!(
                "rules:\n",
                "  - name: child repair\n",
                "    check: test -f repaired\n",
                "    fix: touch repaired\n",
                "patterns:\n",
                "  - '*.sh'\n",
                "  - '!skip.sh'\n"
            ),
        )
        .unwrap();
        let config = root.join("root.yaml");
        std::fs::write(
            &config,
            concat!(
                "rules:\n",
                "  - name: root\n",
                "    check: test -f root-marker\n",
                "  - remote: child/child.yaml\n",
                "patterns:\n",
                "  - 'root.sh'\n"
            ),
        )
        .unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            vec![
                format!("--config={}", config.display()),
                "check".to_string(),
                "--fix".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(
            code,
            0,
            "stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        assert!(child.join("repaired").is_file());
        assert!(!root.join("repaired").exists());
        assert!(root.join("root-pattern-ran").is_file());
        assert!(child.join("child-pattern-ran").is_file());
        assert!(!child.join("skipped-pattern-ran").exists());
    }

    #[test]
    fn test_cli_executes_root_and_remote_pattern_only_definitions() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join("root");
        let child = root.join("child");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(root.join("root.sh"), "touch root-pattern-ran\n").unwrap();
        std::fs::write(child.join("child.sh"), "touch child-pattern-ran\n").unwrap();
        std::fs::write(child.join("child.yaml"), "patterns:\n  - child.sh\n").unwrap();
        let config = root.join("root.yaml");
        std::fs::write(
            &config,
            concat!(
                "rules:\n",
                "  - remote: child/child.yaml\n",
                "patterns:\n",
                "  - root.sh\n"
            ),
        )
        .unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            vec![
                format!("--config={}", config.display()),
                "check".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(
            code,
            0,
            "stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        assert!(root.join("root-pattern-ran").is_file());
        assert!(child.join("child-pattern-ran").is_file());
    }

    fn initialize_local_git_repository(
        path: &std::path::Path,
        config: &str,
        marker: &str,
    ) -> String {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join(".checksy.yaml"), config).unwrap();
        std::fs::write(path.join(marker), marker).unwrap();

        let run_git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(path)
                .args(args)
                .env("GIT_AUTHOR_NAME", "Checksy Test")
                .env("GIT_AUTHOR_EMAIL", "checksy@example.invalid")
                .env("GIT_COMMITTER_NAME", "Checksy Test")
                .env("GIT_COMMITTER_EMAIL", "checksy@example.invalid")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        };
        run_git(&["init", "--initial-branch=main"]);
        run_git(&["add", "."]);
        run_git(&["commit", "-m", "fixture"]);

        format!("file://{}", path.display())
    }

    #[test]
    fn test_install_and_check_fix_discover_nested_git_dependencies() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_b_path = temp.path().join("repo-b");
        let repo_b = initialize_local_git_repository(
            &repo_b_path,
            "rules:\n  - name: repository b\n    check: test -f b.marker\n",
            "b.marker",
        );
        let repo_a_path = temp.path().join("repo-a");
        let repo_a_config = format!(
            concat!(
                "cachePath: nested-cache-must-be-ignored\n",
                "rules:\n",
                "  - name: repository a\n",
                "    check: test -f a.marker\n",
                "  - remote: 'git+{}#main:.checksy.yaml'\n"
            ),
            repo_b
        );
        let repo_a = initialize_local_git_repository(&repo_a_path, &repo_a_config, "a.marker");

        let install_root = temp.path().join("install-root");
        std::fs::create_dir(&install_root).unwrap();
        let install_config = install_root.join("root.yaml");
        std::fs::write(
            &install_config,
            format!(
                "cachePath: cache\nrules:\n  - remote: 'git+{}#main:.checksy.yaml'\n",
                repo_a
            ),
        )
        .unwrap();

        let mut install_stdout = Vec::new();
        let mut install_stderr = Vec::new();
        let install_code = run(
            vec![
                format!("--config={}", install_config.display()),
                "install".to_string(),
            ],
            &mut install_stdout,
            &mut install_stderr,
        );
        assert_eq!(
            install_code,
            0,
            "stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&install_stdout),
            String::from_utf8_lossy(&install_stderr)
        );
        let install_cache = CacheManager::new(&install_root, Some("cache"));
        assert!(install_cache.is_cached(&repo_a, "main"));
        assert!(install_cache.is_cached(&repo_b, "main"));

        let mut check_stdout = Vec::new();
        let mut check_stderr = Vec::new();
        let check_code = run(
            vec![
                format!("--config={}", install_config.display()),
                "check".to_string(),
            ],
            &mut check_stdout,
            &mut check_stderr,
        );
        assert_eq!(
            check_code,
            0,
            "stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&check_stdout),
            String::from_utf8_lossy(&check_stderr)
        );

        let fix_root = temp.path().join("fix-root");
        std::fs::create_dir(&fix_root).unwrap();
        let fix_config = fix_root.join("root.yaml");
        std::fs::write(
            &fix_config,
            format!(
                "cachePath: cache\nrules:\n  - remote: 'git+{}#main:.checksy.yaml'\n",
                repo_a
            ),
        )
        .unwrap();

        let mut fix_stdout = Vec::new();
        let mut fix_stderr = Vec::new();
        let fix_code = run(
            vec![
                format!("--config={}", fix_config.display()),
                "check".to_string(),
                "--fix".to_string(),
            ],
            &mut fix_stdout,
            &mut fix_stderr,
        );
        assert_eq!(
            fix_code,
            0,
            "stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&fix_stdout),
            String::from_utf8_lossy(&fix_stderr)
        );
        let fix_cache = CacheManager::new(&fix_root, Some("cache"));
        assert!(fix_cache.is_cached(&repo_a, "main"));
        assert!(fix_cache.is_cached(&repo_b, "main"));
    }

    #[test]
    fn test_install_prune_keeps_an_in_use_ref_with_a_slash() {
        let temp = tempfile::TempDir::new().unwrap();
        let source_path = temp.path().join("source-with-feature");
        let source = initialize_local_git_repository(
            &source_path,
            "rules:\n  - check: 'true'\n",
            "source.marker",
        );
        let branch_status = std::process::Command::new("git")
            .arg("-C")
            .arg(&source_path)
            .args(["branch", "feature/origin-aware"])
            .status()
            .unwrap();
        assert!(branch_status.success());

        let root = temp.path().join("root-with-prune");
        std::fs::create_dir(&root).unwrap();
        let config = root.join("root.yaml");
        std::fs::write(
            &config,
            format!(
                "cachePath: cache\nrules:\n  - remote: 'git+{}#feature/origin-aware:.checksy.yaml'\n",
                source
            ),
        )
        .unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            vec![
                format!("--config={}", config.display()),
                "install".to_string(),
                "--prune".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(
            code,
            0,
            "stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        let cache = CacheManager::new(&root, Some("cache"));
        assert!(cache.is_cached(&source, "feature/origin-aware"));
    }

    #[cfg(unix)]
    #[test]
    fn test_install_and_check_fix_reject_symlinked_cache_ancestors_before_mutation() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::TempDir::new().unwrap();
        let source_path = temp.path().join("source");
        let source = initialize_local_git_repository(
            &source_path,
            "rules:\n  - check: 'true'\n",
            "source.marker",
        );

        let install_root = temp.path().join("install-root");
        std::fs::create_dir(&install_root).unwrap();
        let install_config = install_root.join("root.yaml");
        std::fs::write(
            &install_config,
            format!(
                "cachePath: cache\nrules:\n  - remote: 'git+{}#main:.checksy.yaml'\n",
                source
            ),
        )
        .unwrap();
        let install_cache = CacheManager::new(&install_root, Some("cache"));
        let install_slot = install_cache.ref_cache_path(&source, "main");
        let external_repo_directory = temp.path().join("external-install");
        let external_checkout = external_repo_directory.join("main");
        initialize_local_git_repository(
            &external_checkout,
            "rules:\n  - check: 'true'\n",
            "must-survive.marker",
        );
        std::fs::create_dir_all(install_slot.parent().unwrap().parent().unwrap()).unwrap();
        symlink(&external_repo_directory, install_slot.parent().unwrap()).unwrap();

        let mut install_stdout = Vec::new();
        let mut install_stderr = Vec::new();
        let install_code = run(
            vec![
                format!("--config={}", install_config.display()),
                "install".to_string(),
            ],
            &mut install_stdout,
            &mut install_stderr,
        );
        assert_eq!(install_code, 2);
        assert!(external_checkout.join("must-survive.marker").is_file());
        assert!(String::from_utf8_lossy(&install_stderr).contains("symbolic link"));

        let fix_root = temp.path().join("fix-root");
        std::fs::create_dir(&fix_root).unwrap();
        let fix_config = fix_root.join("root.yaml");
        std::fs::write(
            &fix_config,
            format!(
                "cachePath: cache\nrules:\n  - remote: 'git+{}#main:.checksy.yaml'\n",
                source
            ),
        )
        .unwrap();
        let fix_cache = CacheManager::new(&fix_root, Some("cache"));
        let fix_slot = fix_cache.ref_cache_path(&source, "main");
        let external_missing = temp.path().join("external-fix");
        std::fs::create_dir(&external_missing).unwrap();
        std::fs::create_dir_all(fix_slot.parent().unwrap().parent().unwrap()).unwrap();
        symlink(&external_missing, fix_slot.parent().unwrap()).unwrap();

        let mut fix_stdout = Vec::new();
        let mut fix_stderr = Vec::new();
        let fix_code = run(
            vec![
                format!("--config={}", fix_config.display()),
                "check".to_string(),
                "--fix".to_string(),
            ],
            &mut fix_stdout,
            &mut fix_stderr,
        );
        assert_eq!(fix_code, 2);
        assert!(!external_missing.join("main").exists());
        assert!(String::from_utf8_lossy(&fix_stderr).contains("symbolic link"));
    }
}
