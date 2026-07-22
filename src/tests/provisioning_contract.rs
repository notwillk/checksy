use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const README: &str = include_str!("../../README.md");
const ARCHITECTURE: &str = include_str!("../../ARCHITECTURE.md");
const STRICT_FIXTURES: &str = include_str!("../../fixtures/strict-config/README.md");

fn checksy() -> Command {
    Command::new(env!("CARGO_BIN_EXE_checksy"))
}

fn run(args: &[&str]) -> Output {
    checksy().args(args).output().unwrap()
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("checksy exited by signal")
}

fn write_config(path: &Path, check: &str) {
    fs::write(
        path,
        format!("rules:\n  - name: contract-test\n    check: '{check}'\n    severity: error\n"),
    )
    .unwrap();
}

#[test]
fn public_help_describes_the_provisioning_cli() {
    let output = run(&["help"]);
    assert_eq!(code(&output), 0);
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("provision the current machine"));
    assert!(stdout.contains("check      Run checks; add --fix to provision the machine"));
    assert!(stdout.contains("schema     Print the generated Draft 7 configuration schema"));
    for command in ["check", "diagnose", "install", "init", "schema", "version"] {
        assert!(stdout.contains(command), "help omitted {command}");
    }
    assert!(!stdout.contains("apply"));
    assert!(stdout.contains("interactive-fix"));
    assert!(stdout.contains("--non-interactive"));
    assert_eq!(
        env!("CARGO_PKG_DESCRIPTION"),
        "Provision the current machine from trusted configuration"
    );
}

#[test]
fn documentation_describes_the_implemented_strict_configuration_boundary() {
    for expected in [
        "strictly decodes the complete",
        "Top-level and rule objects are closed",
        "Every rule is exactly one of these forms",
        "Stdin documents must be self-contained",
        "strict configuration corpus",
    ] {
        assert!(README.contains(expected), "README omitted: {expected}");
    }

    for expected in [
        "## Configuration boundary",
        "All configuration entry paths share one strict typed decoder",
        "generate a deterministic Draft 7 schema",
        "strict configuration corpus",
        "### Configuration test coverage",
    ] {
        assert!(
            ARCHITECTURE.contains(expected),
            "architecture omitted: {expected}"
        );
    }

    for expected in [
        "structural",
        "yaml-parser",
        "runtime-only",
        "The two supported rule forms are deliberately exact",
    ] {
        assert!(
            STRICT_FIXTURES.contains(expected),
            "strict fixture README omitted: {expected}"
        );
    }
}

#[test]
fn public_cli_preserves_the_stable_implemented_exit_classes() {
    assert_eq!(code(&run(&[])), 1);
    assert_eq!(code(&run(&["unknown-command"])), 2);

    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.yaml");
    let missing = missing.to_str().unwrap();
    assert_eq!(code(&run(&["--config", missing, "check"])), 2);
    assert_eq!(code(&run(&["--config", missing, "check", "--no-fail"])), 2);

    let passing = directory.path().join("passing.yaml");
    write_config(&passing, "true");
    let passing = passing.to_str().unwrap();
    let output = run(&["--config", passing, "check"]);
    assert_eq!(code(&output), 0);
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("All rules validated"));

    let failing = directory.path().join("failing.yaml");
    write_config(&failing, "false");
    let failing = failing.to_str().unwrap();
    let output = run(&["--config", failing, "check"]);
    assert_eq!(code(&output), 3);
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("rules failed validation"));

    let output = run(&["--config", failing, "check", "--no-fail"]);
    assert_eq!(code(&output), 0);
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("rules failed validation"));
}

#[test]
fn documentation_distinguishes_the_runner_from_the_remaining_target() {
    for expected in [
        "checksy provisions the current machine",
        "`checksy check --fix` is Checksy's only provisioning lifecycle",
        "Checksy intentionally executes arbitrary Bash",
        "Fetching, updating, authenticating,",
        "`interactive-fix`",
        "`--non-interactive`",
        "provisioning lock",
        "<account-home>/.local/state/checksy/provision.lock",
        "<account-home>/Library/Application Support/checksy/provision.lock",
        "/var/lib/checksy/provision.lock",
        "/Library/Application Support/checksy/provision.lock",
        "| `4` | Provisioning lock contention; reserved until locking is implemented |",
        "`--no-fail` masks only rule-compliance exit `3`",
        "### Command supervision",
        "process-runner fixture",
        "Rules without `timeout` use `15m`",
    ] {
        assert!(README.contains(expected), "README omitted: {expected}");
    }

    for expected in [
        "checksy is a CLI provisioner",
        "## Security and mutation boundary",
        "## Normative P0 execution contract",
        "This section records both implemented behavior and the remaining target",
        "The lock namespace is `checksy-provision`, keyed only by effective UID",
        "this is not a cross-UID machine-global",
        "| `4` | Provisioning lock contention; reserved until the lock is implemented |",
        "`--no-fail` affects only exit `3`",
        "timeout, child-signal, and supervision failures are implemented operational",
        "### Supervised command lifecycle",
        "### Process-supervision test coverage",
    ] {
        assert!(
            ARCHITECTURE.contains(expected),
            "architecture omitted: {expected}"
        );
    }
}
