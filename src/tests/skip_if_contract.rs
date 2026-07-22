use serde::Deserialize;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

mod support;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SkipIfCorpus {
    schema_version: u64,
    cases: Vec<SkipIfCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SkipIfCase {
    id: String,
    fixture: String,
    assets: Vec<String>,
    modes: Vec<String>,
    test: String,
}

const EXECUTABLE_TESTS: &[&str] = &[
    "skip_exit_zero_suppresses_checks_and_both_repair_forms",
    "completed_nonzero_predicates_run_checks_without_leaking_output",
    "predicate_runs_once_before_check_and_fix_recheck_cycle",
    "preconditions_and_ordinary_rules_are_retained_as_skips",
    "severity_filtering_precedes_predicates_and_skipped_errors_do_not_fail",
    "all_configuration_entrypoints_and_deprecated_diagnose_apply_skip_if",
    "environment_and_command_availability_gates_use_inherited_environment",
    "predicate_stdin_is_dev_null_in_check_and_fix_modes",
    "predicate_timeout_is_bounded_operational_and_fail_fast",
    "predicate_child_signal_is_operational_and_fail_fast",
    "bash_spawn_failure_is_operational_fail_fast_and_not_masked",
];

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join("skip-if")
}

fn corpus() -> SkipIfCorpus {
    let data = fs::read_to_string(fixture_root().join("cases.yaml")).unwrap();
    let corpus: SkipIfCorpus = serde_yaml::from_str(&data).unwrap();
    assert_eq!(corpus.schema_version, 1);
    corpus
}

fn case<'a>(corpus: &'a SkipIfCorpus, id: &str) -> &'a SkipIfCase {
    corpus
        .cases
        .iter()
        .find(|case| case.id == id)
        .unwrap_or_else(|| panic!("skip-if corpus omitted {id:?}"))
}

fn collect_files(directory: &Path, root: &Path, files: &mut BTreeSet<String>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_files(&path, root, files);
        } else {
            files.insert(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_directory(&source_path, &destination_path);
        } else {
            fs::copy(source_path, destination_path).unwrap();
        }
    }
}

fn writable_fixture_copy() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    copy_directory(&fixture_root(), directory.path());
    directory
}

fn checksy() -> Command {
    Command::new(env!("CARGO_BIN_EXE_checksy"))
}

fn capture(mut command: Command, stdin: Option<&[u8]>, provisioning: bool) -> Output {
    let _provisioning_guard = provisioning.then(support::provisioning_test_guard);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(input) = stdin {
        command.stdin(Stdio::piped());
        let mut child = command.spawn().unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    } else {
        command.stdin(Stdio::null()).output().unwrap()
    }
}

fn file_command(path: &Path, subcommand: &str, extra: &[&str]) -> Command {
    let mut command = checksy();
    command
        .arg("--config")
        .arg(path)
        .arg(subcommand)
        .args(extra);
    command
}

fn run_file(path: &Path, subcommand: &str, extra: &[&str]) -> Output {
    let provisioning = extra.contains(&"--fix");
    capture(file_command(path, subcommand, extra), None, provisioning)
}

fn output_preview(bytes: &[u8]) -> String {
    const EDGE: usize = 400;
    if bytes.len() <= EDGE * 2 {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    format!(
        "{}\n... {} bytes omitted from test failure preview ...\n{}",
        String::from_utf8_lossy(&bytes[..EDGE]),
        bytes.len() - EDGE * 2,
        String::from_utf8_lossy(&bytes[bytes.len() - EDGE..])
    )
}

fn assert_exit(label: &str, output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{label}: stdout={} stderr={}",
        output_preview(&output.stdout),
        output_preview(&output.stderr)
    );
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn combined(output: &Output) -> String {
    format!("{}{}", stdout(output), stderr(output))
}

fn assert_absent(directory: &Path, names: &[&str]) {
    for name in names {
        assert!(
            !directory.join(name).exists(),
            "unexpected marker {}",
            directory.join(name).display()
        );
    }
}

#[test]
fn corpus_is_closed_unique_network_free_and_mapped_to_tests() {
    let corpus = corpus();
    let ids: BTreeSet<_> = corpus.cases.iter().map(|case| case.id.as_str()).collect();
    assert_eq!(ids.len(), corpus.cases.len(), "duplicate case ID");

    let fixture_paths: BTreeSet<_> = corpus
        .cases
        .iter()
        .map(|case| case.fixture.as_str())
        .collect();
    assert_eq!(
        fixture_paths.len(),
        corpus.cases.len(),
        "a configuration is assigned to more than one corpus case"
    );

    let known_tests: BTreeSet<_> = EXECUTABLE_TESTS.iter().copied().collect();
    let indexed_tests: BTreeSet<_> = corpus.cases.iter().map(|case| case.test.as_str()).collect();
    assert_eq!(indexed_tests, known_tests);

    let known_modes = BTreeSet::from([
        "auto-discovery",
        "config-dash",
        "deprecated-diagnose",
        "file-check",
        "file-check-empty-path-no-fail",
        "file-check-gates-absent",
        "file-check-gates-present",
        "file-check-no-fail",
        "file-check-severity",
        "file-fix",
        "stdin-config",
    ]);
    for case in &corpus.cases {
        assert!(!case.modes.is_empty(), "{} has no execution mode", case.id);
        for mode in &case.modes {
            assert!(
                known_modes.contains(mode.as_str()),
                "{} has unknown mode {mode:?}",
                case.id
            );
        }
    }

    let root = fixture_root();
    let mut actual_files = BTreeSet::new();
    collect_files(&root, &root, &mut actual_files);
    actual_files.remove("README.md");
    actual_files.remove("cases.yaml");

    let indexed_files: BTreeSet<_> = corpus
        .cases
        .iter()
        .flat_map(|case| std::iter::once(&case.fixture).chain(case.assets.iter()))
        .cloned()
        .collect();
    assert_eq!(indexed_files, actual_files);

    for path in actual_files {
        let bytes = fs::read(root.join(&path)).unwrap();
        assert!(!bytes.contains(&b'\r'), "{path} must use LF endings");
        assert_eq!(
            bytes.last(),
            Some(&b'\n'),
            "{path} needs a terminal newline"
        );
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("http://"), "{path} must remain network-free");
        assert!(
            !text.contains("https://"),
            "{path} must remain network-free"
        );

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if path.starts_with("bin/") {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                fs::metadata(root.join(&path)).unwrap().permissions().mode() & 0o111,
                0,
                "{path} must be executable"
            );
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn skip_exit_zero_suppresses_checks_and_both_repair_forms() {
    let corpus = corpus();
    let case = case(&corpus, "skip-zero-repairs");

    for (label, extra) in [("check", &[][..]), ("fix", &["--fix"][..])] {
        let copy = writable_fixture_copy();
        let output = run_file(&copy.path().join(&case.fixture), "check", extra);
        assert_exit(label, &output, 0);
        assert_eq!(
            fs::read_to_string(copy.path().join(".trace")).unwrap(),
            "ordinary-predicate\ninteractive-predicate\n"
        );
        assert_absent(
            copy.path(),
            &[
                ".unexpected-ordinary-check",
                ".unexpected-ordinary-fix",
                ".unexpected-interactive-check",
                ".unexpected-interactive-fix",
            ],
        );

        let stdout = stdout(&output);
        assert!(stdout.contains("⏭️ skipped ordinary repair (skipped)"));
        assert!(stdout.contains("⏭️ skipped interactive repair (skipped)"));
        assert!(stdout.contains("😎 All applicable rules validated; 2 skipped"));
        let combined = combined(&output);
        assert!(!combined.contains("hidden zero ordinary"));
        assert!(!combined.contains("hidden zero interactive"));
        assert!(!combined.contains("interactive repair required"));
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn completed_nonzero_predicates_run_checks_without_leaking_output() {
    let corpus = corpus();
    let case = case(&corpus, "completed-nonzero");
    let copy = writable_fixture_copy();
    let output = run_file(&copy.path().join(&case.fixture), "check", &[]);
    assert_exit(&case.id, &output, 0);

    assert_eq!(
        fs::read_to_string(copy.path().join(".trace")).unwrap(),
        "exit-1-check\nexit-23-check\nexit-127-check\n"
    );
    assert!(stderr(&output).is_empty());
    let stdout = stdout(&output);
    assert!(stdout.contains("✅ predicate exits one"));
    assert!(stdout.contains("✅ predicate exits twenty-three"));
    assert!(stdout.contains("✅ predicate reaches shell-level 127"));
    assert!(stdout.ends_with("😎 All rules validated\n"));
    assert!(!stdout.contains("(skipped)"));
    let combined = combined(&output);
    for hidden in [
        "hidden exit one",
        "hidden exit twenty-three",
        "hidden exit one-twenty-seven",
        "checksy-command-that-must-not-exist-for-skip-if-contract",
    ] {
        assert!(
            !combined.contains(hidden),
            "predicate output leaked: {hidden}"
        );
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn predicate_runs_once_before_check_and_fix_recheck_cycle() {
    let corpus = corpus();
    let case = case(&corpus, "once-fix-recheck");

    let check_copy = writable_fixture_copy();
    let output = run_file(&check_copy.path().join(&case.fixture), "check", &[]);
    assert_exit("once check-only", &output, 3);
    assert_eq!(
        fs::read_to_string(check_copy.path().join(".trace")).unwrap(),
        "predicate\ncheck\n"
    );
    assert!(!combined(&output).contains("hidden once predicate output"));

    let fix_copy = writable_fixture_copy();
    let output = run_file(&fix_copy.path().join(&case.fixture), "check", &["--fix"]);
    assert_exit("once fix and recheck", &output, 0);
    assert_eq!(
        fs::read_to_string(fix_copy.path().join(".trace")).unwrap(),
        "predicate\ncheck\nfix\ncheck\n"
    );
    assert!(fix_copy.path().join(".fixed").is_file());
    assert!(!combined(&output).contains("hidden once predicate output"));
    assert!(stdout(&output).ends_with("😎 All rules validated\n"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn preconditions_and_ordinary_rules_are_retained_as_skips() {
    let corpus = corpus();
    let case = case(&corpus, "preconditions-and-rules");
    let copy = writable_fixture_copy();
    let output = run_file(&copy.path().join(&case.fixture), "check", &[]);
    assert_exit(&case.id, &output, 0);

    assert!(copy.path().join(".precondition-predicate").is_file());
    assert!(copy.path().join(".rule-predicate").is_file());
    assert_absent(
        copy.path(),
        &[".unexpected-precondition-check", ".unexpected-rule-check"],
    );
    let stdout = stdout(&output);
    let precondition = stdout.find("⏭️ skipped precondition (skipped)").unwrap();
    let rule = stdout.find("⏭️ skipped ordinary rule (skipped)").unwrap();
    assert!(precondition < rule);
    assert!(stdout.ends_with("😎 All applicable rules validated; 2 skipped\n"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn severity_filtering_precedes_predicates_and_skipped_errors_do_not_fail() {
    let corpus = corpus();
    let case = case(&corpus, "severity-filtering");
    let copy = writable_fixture_copy();
    let output = run_file(
        &copy.path().join(&case.fixture),
        "check",
        &["--check-severity", "warn", "--fail-severity", "warn"],
    );
    assert_exit(&case.id, &output, 3);

    assert_absent(
        copy.path(),
        &[
            ".unexpected-filtered-predicate",
            ".unexpected-filtered-check",
            ".unexpected-high-severity-check",
        ],
    );
    assert!(copy.path().join(".high-severity-predicate").is_file());
    assert!(copy.path().join(".warning-check").is_file());
    let stdout = stdout(&output);
    assert!(!stdout.contains("filtered info rule"));
    assert!(stdout.contains("⏭️ skipped error rule (skipped)"));
    assert!(stdout.contains("❌ failing warning rule"));
    assert!(stdout.contains("😭 1 rules failed validation; 1 skipped"));
    assert!(stdout.contains("- failing warning rule"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn all_configuration_entrypoints_and_deprecated_diagnose_apply_skip_if() {
    let corpus = corpus();
    let file_case = case(&corpus, "explicit-and-stdin-entrypoints");
    let auto_case = case(&corpus, "auto-discovered-entrypoint");
    let copy = writable_fixture_copy();
    let config = copy.path().join(&file_case.fixture);
    let config_bytes = fs::read(&config).unwrap();

    let mut runs: Vec<(&str, PathBuf, Output)> = Vec::new();

    let mut command = file_command(&config, "check", &[]);
    command.env("CHECKSY_SKIP_IF_ENTRYPOINT", "explicit-file");
    runs.push((
        "explicit-file",
        copy.path().to_path_buf(),
        capture(command, None, false),
    ));

    let auto_directory = copy.path().join("autodiscovery");
    let mut command = checksy();
    command
        .arg("check")
        .current_dir(&auto_directory)
        .env("CHECKSY_SKIP_IF_ENTRYPOINT", "auto-discovery");
    runs.push((
        "auto-discovery",
        auto_directory.clone(),
        capture(command, None, false),
    ));

    let mut command = checksy();
    command
        .args(["--stdin-config", "check"])
        .current_dir(copy.path())
        .env("CHECKSY_SKIP_IF_ENTRYPOINT", "stdin-config");
    runs.push((
        "stdin-config",
        copy.path().to_path_buf(),
        capture(command, Some(&config_bytes), false),
    ));

    let mut command = checksy();
    command
        .args(["--config", "-", "check"])
        .current_dir(copy.path())
        .env("CHECKSY_SKIP_IF_ENTRYPOINT", "config-dash");
    runs.push((
        "config-dash",
        copy.path().to_path_buf(),
        capture(command, Some(&config_bytes), false),
    ));

    let mut command = file_command(&config, "diagnose", &[]);
    command.env("CHECKSY_SKIP_IF_ENTRYPOINT", "diagnose");
    runs.push((
        "diagnose",
        copy.path().to_path_buf(),
        capture(command, None, false),
    ));

    assert_eq!(auto_case.fixture, "autodiscovery/.checksy.yaml");
    for (mode, marker_directory, output) in runs {
        assert_exit(mode, &output, 0);
        assert!(
            marker_directory
                .join(format!(".{mode}.predicate"))
                .is_file(),
            "{mode} did not run the predicate in its check workdir"
        );
        assert!(
            !marker_directory
                .join(format!(".unexpected-{mode}-check"))
                .exists(),
            "{mode} unexpectedly ran the check"
        );
        let stdout = stdout(&output);
        assert!(stdout.contains("⏭️ entrypoint predicate (skipped)"));
        assert!(stdout.ends_with("😎 All applicable rules validated; 1 skipped\n"));
        if mode == "diagnose" {
            assert!(stderr(&output).contains("is deprecated"));
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn environment_and_command_availability_gates_use_inherited_environment() {
    let corpus = corpus();
    let case = case(&corpus, "inherited-environment-gates");

    let present_copy = writable_fixture_copy();
    let binary_directory = present_copy.path().join("bin");
    let current_path = std::env::var_os("PATH").unwrap_or_default();
    let path_entries = std::iter::once(binary_directory.as_os_str().to_os_string())
        .chain(std::env::split_paths(&current_path).map(|path| path.into_os_string()));
    let path = std::env::join_paths(path_entries).unwrap();
    let mut command = file_command(&present_copy.path().join(&case.fixture), "check", &[]);
    command
        .env("CHECKSY_SKIP_IF_ENV_GATE", "enabled")
        .env("PATH", path);
    let output = capture(command, None, false);
    assert_exit("present gates", &output, 0);
    assert!(!present_copy.path().join(".trace").exists());
    assert!(stdout(&output).ends_with("😎 All applicable rules validated; 2 skipped\n"));

    let absent_copy = writable_fixture_copy();
    let mut command = file_command(&absent_copy.path().join(&case.fixture), "check", &[]);
    command.env_remove("CHECKSY_SKIP_IF_ENV_GATE");
    let output = capture(command, None, false);
    assert_exit("absent gates", &output, 0);
    assert_eq!(
        fs::read_to_string(absent_copy.path().join(".trace")).unwrap(),
        "environment-check\ncommand-check\n"
    );
    assert!(stdout(&output).ends_with("😎 All rules validated\n"));
    assert!(!stdout(&output).contains("(skipped)"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn predicate_stdin_is_dev_null_in_check_and_fix_modes() {
    let corpus = corpus();
    let case = case(&corpus, "predicate-stdin-eof");
    for (label, extra) in [("check", &[][..]), ("fix", &["--fix"][..])] {
        let copy = writable_fixture_copy();
        let command = file_command(&copy.path().join(&case.fixture), "check", extra);
        let output = capture(
            command,
            Some(b"outer stdin must not reach predicate\n"),
            !extra.is_empty(),
        );
        assert_exit(label, &output, 0);
        assert!(copy.path().join(".predicate-eof").is_file());
        assert_absent(
            copy.path(),
            &[
                ".unexpected-predicate-input",
                ".unexpected-eof-check",
                ".unexpected-eof-fix",
            ],
        );
        assert!(stdout(&output).contains("⏭️ predicate receives dev null (skipped)"));
        assert!(!combined(&output).contains("unexpected predicate stdin"));
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn predicate_timeout_is_bounded_operational_and_fail_fast() {
    let corpus = corpus();
    let case = case(&corpus, "predicate-timeout");
    let copy = writable_fixture_copy();
    let output = run_file(&copy.path().join(&case.fixture), "check", &["--no-fail"]);
    assert_exit(&case.id, &output, 2);
    assert_absent(
        copy.path(),
        &[".unexpected-timeout-check", ".unexpected-after-timeout"],
    );

    let stderr = stderr(&output);
    assert!(stderr.contains("predicate timeout skip-if"));
    assert!(stderr.contains("predicate timeout retained stderr"));
    assert!(stderr.contains("bytes omitted from bounded process output"));
    assert!(stderr.contains('Z'));
    assert!(stderr.to_ascii_lowercase().contains("timed out"));
    assert!(
        output.stderr.len() < 1_100_000,
        "operational output was not bounded: {} bytes",
        output.stderr.len()
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn predicate_child_signal_is_operational_and_fail_fast() {
    let corpus = corpus();
    let case = case(&corpus, "predicate-child-signal");
    let copy = writable_fixture_copy();
    let output = run_file(&copy.path().join(&case.fixture), "check", &["--no-fail"]);
    assert_exit(&case.id, &output, 2);
    assert_absent(
        copy.path(),
        &[".unexpected-signal-check", ".unexpected-after-signal"],
    );
    let stderr = stderr(&output);
    assert!(stderr.contains("predicate child signal skip-if"));
    assert!(stderr.contains("predicate signal retained stdout"));
    assert!(stderr.contains("predicate signal retained stderr"));
    assert!(stderr.to_ascii_lowercase().contains("signal"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn bash_spawn_failure_is_operational_fail_fast_and_not_masked() {
    let corpus = corpus();
    let case = case(&corpus, "predicate-bash-spawn-failure");
    let copy = writable_fixture_copy();
    let mut command = file_command(&copy.path().join(&case.fixture), "check", &["--no-fail"]);
    command.env("PATH", OsString::new());
    let output = capture(command, None, false);
    assert_exit(&case.id, &output, 2);
    assert_absent(
        copy.path(),
        &[".unexpected-spawn-check", ".unexpected-after-spawn-failure"],
    );
    let stderr = stderr(&output);
    assert!(stderr.contains("predicate Bash spawn failure skip-if"));
    assert!(stderr.contains("failed to spawn process"));
}
