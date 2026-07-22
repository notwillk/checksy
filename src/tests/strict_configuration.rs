use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::collections::BTreeSet;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StrictConfigCorpus {
    schema_version: u64,
    cases: Vec<StrictConfigCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StrictConfigCase {
    id: String,
    fixture: String,
    expected: String,
    validation_layer: String,
    #[serde(default)]
    error_contains: Option<String>,
}

fn checksy() -> Command {
    Command::new(env!("CARGO_BIN_EXE_checksy"))
}

fn exit_code(output: &Output) -> i32 {
    output.status.code().unwrap_or_else(|| {
        panic!(
            "checksy exited by signal: stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn output_context(output: &Output) -> String {
    format!(
        "exit={:?} stdout={:?} stderr={:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn run(args: &[&str]) -> Output {
    checksy().args(args).output().unwrap()
}

fn run_with_path_env(args: &[&str], variables: &[(&str, &Path)]) -> Output {
    let mut command = checksy();
    command.args(args);
    for (name, value) in variables {
        command.env(name, value);
    }
    command.output().unwrap()
}

fn run_with_stdin(args: &[&str], input: &[u8]) -> Output {
    run_with_stdin_and_path_env(args, input, &[])
}

fn run_with_stdin_and_path_env(args: &[&str], input: &[u8], variables: &[(&str, &Path)]) -> Output {
    let mut command = checksy();
    command.args(args);
    for (name, value) in variables {
        command.env(name, value);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join("strict-config")
}

fn corpus() -> StrictConfigCorpus {
    let data = fs::read_to_string(fixture_root().join("cases.yaml")).unwrap();
    let corpus: StrictConfigCorpus = serde_yaml::from_str(&data).unwrap();
    assert_eq!(corpus.schema_version, 1);
    corpus
}

fn case_data(case: &StrictConfigCase) -> Vec<u8> {
    fs::read(fixture_root().join(&case.fixture)).unwrap()
}

fn is_accepted(case: &StrictConfigCase) -> bool {
    match case.expected.as_str() {
        "accept" => true,
        "reject" => false,
        other => panic!("{} has unsupported expectation {other:?}", case.id),
    }
}

fn assert_expected_cli_result(case: &StrictConfigCase, output: &Output, source: &str) {
    let expected = if is_accepted(case) { 0 } else { 2 };
    assert_eq!(
        exit_code(output),
        expected,
        "{} ({source}) disagreed with corpus expectation: {}",
        case.id,
        output_context(output)
    );

    if !is_accepted(case) {
        if let Some(expected_error) = &case.error_contains {
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains(expected_error),
                "{} ({source}) stderr {stderr:?} omitted {expected_error:?}",
                case.id
            );
        }
    }
}

fn is_file_cli_case(case: &StrictConfigCase) -> bool {
    !is_accepted(case) || case.id != "legacy-git-include"
}

fn is_stdin_cli_case(case: &StrictConfigCase) -> bool {
    !is_accepted(case)
        || !matches!(
            case.id.as_str(),
            "complete-config" | "legacy-git-include" | "precondition-include"
        )
}

fn schema_output() -> Output {
    run(&["schema"])
}

fn compiled_schema() -> (JsonValue, jsonschema::Validator) {
    let output = schema_output();
    assert_eq!(exit_code(&output), 0, "{}", output_context(&output));
    assert!(output.stderr.is_empty(), "{}", output_context(&output));
    let schema: JsonValue = serde_json::from_slice(&output.stdout).unwrap();
    jsonschema::draft7::meta::validate(&schema).expect("checksy schema must be valid Draft 7");
    let validator =
        jsonschema::draft7::new(&schema).expect("checksy schema must compile as Draft 7");
    (schema, validator)
}

fn collect_yaml_paths(directory: &Path, root: &Path, paths: &mut BTreeSet<String>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_yaml_paths(&path, root, paths);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("yaml") {
            paths.insert(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

#[test]
fn corpus_index_is_closed_unique_and_has_exact_layer_exceptions() {
    let corpus = corpus();
    let ids: BTreeSet<_> = corpus.cases.iter().map(|case| case.id.as_str()).collect();
    let paths: BTreeSet<_> = corpus
        .cases
        .iter()
        .map(|case| case.fixture.as_str())
        .collect();
    assert_eq!(ids.len(), corpus.cases.len(), "duplicate corpus case ID");
    assert_eq!(
        paths.len(),
        corpus.cases.len(),
        "duplicate corpus fixture path"
    );

    let root = fixture_root();
    let mut files = BTreeSet::new();
    collect_yaml_paths(&root.join("valid"), &root, &mut files);
    collect_yaml_paths(&root.join("invalid"), &root, &mut files);
    assert_eq!(
        paths
            .into_iter()
            .map(str::to_string)
            .collect::<BTreeSet<_>>(),
        files,
        "cases.yaml must index every and only valid/invalid YAML fixture"
    );

    let yaml_parser: BTreeSet<_> = corpus
        .cases
        .iter()
        .filter(|case| case.validation_layer == "yaml-parser")
        .map(|case| case.id.as_str())
        .collect();
    assert_eq!(
        yaml_parser,
        BTreeSet::from([
            "duplicate-remote-key",
            "duplicate-rule-key",
            "duplicate-top-level-key",
            "multiple-documents",
        ])
    );

    let runtime_only: BTreeSet<_> = corpus
        .cases
        .iter()
        .filter(|case| case.validation_layer == "runtime-only")
        .map(|case| case.id.as_str())
        .collect();
    assert_eq!(
        runtime_only,
        BTreeSet::from(["invalid-glob", "timeout-over-limit", "timeout-overflow"])
    );
}

#[test]
fn corpus_cases_run_through_the_compiled_file_cli() {
    for case in corpus().cases {
        let path = fixture_root().join(&case.fixture);
        if !is_file_cli_case(&case) {
            continue;
        }

        let output = run(&["--config", path.to_str().unwrap(), "check", "--no-fail"]);
        assert_expected_cli_result(&case, &output, "file");
    }
}

#[test]
fn self_contained_corpus_cases_run_through_both_compiled_stdin_paths() {
    for case in corpus().cases {
        let data = case_data(&case);
        if !is_stdin_cli_case(&case) {
            continue;
        }

        let stdin_flag = run_with_stdin(&["--stdin-config", "check", "--no-fail"], &data);
        assert_expected_cli_result(&case, &stdin_flag, "--stdin-config");

        let config_dash = run_with_stdin(&["--config", "-", "check", "--no-fail"], &data);
        assert_expected_cli_result(&case, &config_dash, "--config -");
    }
}

#[test]
fn checked_in_valid_file_and_stdin_fixtures_execute_through_the_compiled_binary() {
    let temp = tempfile::tempdir().unwrap();
    let file_marker = temp.path().join("file-marker");
    let file_config = fixture_root().join("integration/file-valid.yaml");
    let file = run_with_path_env(
        &["--config", file_config.to_str().unwrap(), "check"],
        &[("CHECKSY_TEST_MARKER", &file_marker)],
    );
    assert_eq!(exit_code(&file), 0, "{}", output_context(&file));
    assert_eq!(fs::read_to_string(file_marker).unwrap(), "file");

    let document = fs::read(fixture_root().join("integration/stdin-valid.yaml")).unwrap();

    for (index, args) in [
        &["--stdin-config", "check"][..],
        &["--config", "-", "check"][..],
    ]
    .into_iter()
    .enumerate()
    {
        let marker = temp.path().join(format!("stdin-marker-{index}"));
        let output =
            run_with_stdin_and_path_env(args, &document, &[("CHECKSY_TEST_MARKER", &marker)]);
        assert_eq!(exit_code(&output), 0, "{}", output_context(&output));
        assert_eq!(fs::read_to_string(marker).unwrap(), "stdin");
    }
}

#[test]
fn checked_in_invalid_file_and_stdin_fixtures_are_preflighted_even_with_no_fail() {
    let temp = tempfile::tempdir().unwrap();
    let file_marker = temp.path().join("file-must-not-run");
    let file_config = fixture_root().join("integration/file-invalid.yaml");
    let file = run_with_path_env(
        &[
            "--config",
            file_config.to_str().unwrap(),
            "check",
            "--no-fail",
        ],
        &[("CHECKSY_TEST_MARKER", &file_marker)],
    );
    assert_eq!(exit_code(&file), 2, "{}", output_context(&file));
    assert!(
        !file_marker.exists(),
        "invalid file config executed a command"
    );

    let document = fs::read(fixture_root().join("integration/stdin-invalid.yaml")).unwrap();

    for (index, args) in [
        &["--stdin-config", "check", "--no-fail"][..],
        &["--config", "-", "check", "--no-fail"][..],
    ]
    .into_iter()
    .enumerate()
    {
        let marker = temp.path().join(format!("stdin-must-not-run-{index}"));
        let output =
            run_with_stdin_and_path_env(args, &document, &[("CHECKSY_TEST_MARKER", &marker)]);
        assert_eq!(exit_code(&output), 2, "{}", output_context(&output));
        assert!(!marker.exists(), "invalid stdin config executed a command");
    }
}

#[test]
fn invalid_interactive_fix_shape_is_preflighted_before_any_command() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("interactive-fix-must-not-run");
    let config = fixture_root().join("invalid/both-fix-forms.yaml");
    let output = run_with_path_env(
        &["--config", config.to_str().unwrap(), "check", "--no-fail"],
        &[("CHECKSY_TEST_MARKER", &marker)],
    );

    assert_eq!(exit_code(&output), 2, "{}", output_context(&output));
    assert!(
        !marker.exists(),
        "a command ran before interactive-fix validation completed"
    );
}

#[test]
fn invalid_nested_config_prevents_root_commands() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("root-must-not-run");
    let root = fixture_root().join("integration/nested/root.yaml");
    let output = run_with_path_env(
        &["--config", root.to_str().unwrap(), "check", "--no-fail"],
        &[("CHECKSY_TEST_MARKER", &marker)],
    );
    assert_eq!(exit_code(&output), 2, "{}", output_context(&output));
    assert!(
        !marker.exists(),
        "invalid nested config executed a root command"
    );
}

#[test]
fn invalid_check_fix_config_runs_neither_check_nor_fix() {
    let temp = tempfile::tempdir().unwrap();
    let check_marker = temp.path().join("check-must-not-run");
    let fix_marker = temp.path().join("fix-must-not-run");
    let config = fixture_root().join("integration/fix-invalid.yaml");
    let output = run_with_path_env(
        &[
            "--config",
            config.to_str().unwrap(),
            "check",
            "--fix",
            "--no-fail",
        ],
        &[
            ("CHECKSY_TEST_CHECK_MARKER", &check_marker),
            ("CHECKSY_TEST_FIX_MARKER", &fix_marker),
        ],
    );
    assert_eq!(exit_code(&output), 2, "{}", output_context(&output));
    assert!(
        !check_marker.exists(),
        "invalid config ran its initial check"
    );
    assert!(!fix_marker.exists(), "invalid config ran its fix");
}

#[test]
fn stdin_documents_must_be_self_contained() {
    for document in [
        b"rules:\n  - remote: nested.yaml\n".as_slice(),
        b"preconditions:\n  - remote: nested.yaml\n".as_slice(),
    ] {
        for args in [
            &["--stdin-config", "check", "--no-fail"][..],
            &["--config", "-", "check", "--no-fail"][..],
        ] {
            let output = run_with_stdin(args, document);
            assert_eq!(exit_code(&output), 2, "{}", output_context(&output));
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("stdin") && stderr.contains("remote"),
                "stdin include error was not actionable: {stderr:?}"
            );
        }
    }
}

#[test]
fn automatically_discovered_configuration_uses_strict_loading() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("must-not-run");
    fs::write(
        temp.path().join(".checksy.yaml"),
        format!(
            "rules:\n  - check: 'touch {}'\n    unsupportedField: true\n",
            marker.display()
        ),
    )
    .unwrap();

    let output = checksy()
        .current_dir(temp.path())
        .args(["check", "--no-fail"])
        .output()
        .unwrap();
    assert_eq!(exit_code(&output), 2, "{}", output_context(&output));
    assert!(!marker.exists(), "invalid auto-discovered config executed");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn fake_git_path(temp: &Path, sentinel: &Path) -> (PathBuf, OsString) {
    use std::os::unix::fs::PermissionsExt;

    let bin = temp.join("bin");
    fs::create_dir(&bin).unwrap();
    let git = bin.join("git");
    fs::write(
        &git,
        "#!/bin/sh\n: > \"$CHECKSY_TEST_GIT_SENTINEL\"\nexit 97\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&git).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&git, permissions).unwrap();

    let mut path = OsString::from(bin.as_os_str());
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    assert!(!sentinel.exists());
    (git, path)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn install_rejects_invalid_root_and_nested_configs_before_git() {
    let temp = tempfile::tempdir().unwrap();
    let git_sentinel = temp.path().join("git-must-not-run");
    let (_git, path) = fake_git_path(temp.path(), &git_sentinel);

    let invalid_root = fixture_root().join("integration/install-invalid.yaml");
    let root_output = checksy()
        .env("PATH", &path)
        .env("CHECKSY_TEST_GIT_SENTINEL", &git_sentinel)
        .args(["--config", invalid_root.to_str().unwrap(), "install"])
        .output()
        .unwrap();
    assert_eq!(
        exit_code(&root_output),
        2,
        "{}",
        output_context(&root_output)
    );
    assert!(
        !git_sentinel.exists(),
        "install invoked git for an invalid root"
    );

    let root = temp.path().join("root.yaml");
    let nested = temp.path().join("invalid-nested.yaml");
    fs::write(&root, "rules:\n  - remote: invalid-nested.yaml\n").unwrap();
    fs::write(
        nested,
        concat!(
            "unsupportedField: true\n",
            "rules:\n",
            "  - remote: git+https://example.invalid/config.git\n"
        ),
    )
    .unwrap();
    let nested_output = checksy()
        .env("PATH", path)
        .env("CHECKSY_TEST_GIT_SENTINEL", &git_sentinel)
        .args(["--config", root.to_str().unwrap(), "install"])
        .output()
        .unwrap();
    assert_eq!(
        exit_code(&nested_output),
        2,
        "{}",
        output_context(&nested_output)
    );
    assert!(
        !git_sentinel.exists(),
        "install invoked git for an invalid include"
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_strict_git_include_loads_from_a_fake_cache_without_network() {
    let temp = tempfile::tempdir().unwrap();
    let repo = "https://example.invalid/checks.git";
    let reference = "main";
    let cache = checksy::CacheManager::new(temp.path(), None);
    let checkout = cache.ref_cache_path(repo, reference);
    fs::create_dir_all(checkout.join(".git")).unwrap();
    fs::copy(
        fixture_root().join("integration/git/remote.yaml"),
        checkout.join("remote.yaml"),
    )
    .unwrap();

    let root = temp.path().join("root.yaml");
    fs::copy(fixture_root().join("integration/git/root.yaml"), &root).unwrap();

    let git_sentinel = temp.path().join("git-must-not-run");
    let (_git, path) = fake_git_path(temp.path(), &git_sentinel);
    let output = checksy()
        .env("PATH", path)
        .env("CHECKSY_TEST_GIT_SENTINEL", &git_sentinel)
        .args(["--config", root.to_str().unwrap(), "check"])
        .output()
        .unwrap();
    assert_eq!(exit_code(&output), 0, "{}", output_context(&output));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Cached legacy Git include"),
        "{}",
        output_context(&output)
    );
    assert!(!git_sentinel.exists(), "cached Git check invoked git");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn an_invalid_cached_git_include_fails_before_root_commands_without_network() {
    let temp = tempfile::tempdir().unwrap();
    let repo = "https://example.invalid/checks.git";
    let reference = "main";
    let cache = checksy::CacheManager::new(temp.path(), None);
    let checkout = cache.ref_cache_path(repo, reference);
    fs::create_dir_all(checkout.join(".git")).unwrap();
    fs::copy(
        fixture_root().join("integration/git/remote-invalid.yaml"),
        checkout.join("remote.yaml"),
    )
    .unwrap();

    let root = temp.path().join("root.yaml");
    fs::copy(
        fixture_root().join("integration/git/root-invalid.yaml"),
        &root,
    )
    .unwrap();

    let marker = temp.path().join("root-must-not-run");
    let git_sentinel = temp.path().join("git-must-not-run");
    let (_git, path) = fake_git_path(temp.path(), &git_sentinel);
    let output = checksy()
        .env("PATH", path)
        .env("CHECKSY_TEST_GIT_SENTINEL", &git_sentinel)
        .env("CHECKSY_TEST_MARKER", &marker)
        .args(["--config", root.to_str().unwrap(), "check", "--no-fail"])
        .output()
        .unwrap();

    assert_eq!(exit_code(&output), 2, "{}", output_context(&output));
    assert!(
        !marker.exists(),
        "invalid cached Git config ran a root command"
    );
    assert!(!git_sentinel.exists(), "cached Git check invoked git");
}

#[test]
fn generated_schema_is_deterministic_valid_and_matches_the_fixture_layers() {
    let first = schema_output();
    let second = schema_output();
    assert_eq!(exit_code(&first), 0, "{}", output_context(&first));
    assert_eq!(exit_code(&second), 0, "{}", output_context(&second));
    assert!(first.stderr.is_empty(), "{}", output_context(&first));
    assert!(second.stderr.is_empty(), "{}", output_context(&second));
    assert_eq!(
        first.stdout, second.stdout,
        "schema output is nondeterministic"
    );
    assert!(first.stdout.ends_with(b"\n"));

    let (schema, validator) = compiled_schema();
    assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
    assert_eq!(schema["additionalProperties"], false);

    for case in corpus().cases {
        let data = case_data(&case);
        let typed = serde_yaml::from_slice::<checksy::Config>(&data);
        assert_eq!(
            typed.is_ok(),
            is_accepted(&case),
            "{} disagreed with strict typed deserialization: {:?}",
            case.id,
            typed.err()
        );

        let typed_yaml = serde_yaml::from_slice::<serde_yaml::Value>(&data);
        match case.validation_layer.as_str() {
            "yaml-parser" => {
                assert!(
                    typed_yaml.is_err(),
                    "{} must be rejected by the YAML parser before schema validation",
                    case.id
                );
            }
            "structural" => {
                let instance = serde_json::to_value(typed_yaml.unwrap()).unwrap();
                assert_eq!(
                    validator.is_valid(&instance),
                    is_accepted(&case),
                    "{} disagreed with the generated structural schema",
                    case.id
                );
            }
            "runtime-only" => {
                let instance = serde_json::to_value(typed_yaml.unwrap()).unwrap();
                assert!(
                    validator.is_valid(&instance),
                    "{} must pass structural schema and fail runtime validation",
                    case.id
                );
                assert!(!is_accepted(&case));
            }
            other => panic!("{} has unknown validation layer {other:?}", case.id),
        }
    }
}

#[test]
fn init_emits_a_configuration_accepted_by_the_strict_cli() {
    let temp = tempfile::tempdir().unwrap();
    let init = checksy()
        .current_dir(temp.path())
        .arg("init")
        .output()
        .unwrap();
    assert_eq!(exit_code(&init), 0, "{}", output_context(&init));

    let generated = temp.path().join(".checksy.config.yaml");
    assert!(generated.is_file());
    let check = run(&["--config", generated.to_str().unwrap(), "check"]);
    assert_eq!(exit_code(&check), 0, "{}", output_context(&check));
}
