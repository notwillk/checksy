use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

mod support;

fn rulesy() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rulesy"))
}

fn capture(mut command: Command) -> Output {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap()
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(output.status.code(), Some(expected), "{output:?}");
}

fn confined_command(bundle: &Path, config: &Path) -> Command {
    let mut command = rulesy();
    command
        .arg("--bundle-root")
        .arg(bundle)
        .arg("--config")
        .arg(config)
        .arg("check");
    command
}

fn assert_rejected(output: &Output, fragments: &[&str]) {
    assert_exit(output, 2);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for fragment in fragments {
        assert!(
            stderr.contains(fragment),
            "missing {fragment:?}: {output:?}"
        );
    }
}

fn assert_confined_rejected(bundle: &Path, config: &Path, fragments: &[&str]) {
    assert_rejected(&capture(confined_command(bundle, config)), fragments);
}

fn new_bundle() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let bundle = directory.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    (directory, bundle)
}

fn write_file(path: &Path, contents: impl AsRef<[u8]>) {
    fs::write(path, contents).unwrap();
}

#[cfg(unix)]
#[test]
fn contained_nested_configs_and_patterns_run_from_an_unrelated_directory() {
    use std::os::unix::fs::symlink;

    let _serial = support::provisioning_test_guard();
    let (directory, bundle) = new_bundle();
    let unrelated = tempfile::tempdir().unwrap();
    let config = bundle.join("config");
    for path in [
        config.join("nested"),
        bundle.join("shared"),
        bundle.join("scripts"),
    ] {
        fs::create_dir_all(path).unwrap();
    }
    let bundle_alias = directory.path().join("bundle-alias");
    symlink(&bundle, &bundle_alias).unwrap();

    let root = config.join("root.yaml");
    write_file(
        &root,
        "rules:\n  - remote: nested/child.yaml\npatterns:\n  - ../scripts/root.sh\n",
    );
    write_file(
        &config.join("nested/child.yaml"),
        format!(
            "rules:\n  - remote: '{}'\npatterns:\n  - ../../scripts/child.sh\n",
            bundle_alias.join("shared/sibling.yaml").display()
        ),
    );
    write_file(
        &bundle.join("shared/sibling.yaml"),
        "rules:\n  - name: contained sibling\n    check: test -f sibling.yaml\n",
    );
    write_file(&bundle.join("scripts/root.sh"), "#!/bin/sh\nexit 0\n");
    write_file(&bundle.join("scripts/child.sh"), "#!/bin/sh\nexit 0\n");

    let mut command = rulesy();
    command
        .current_dir(unrelated.path())
        .arg(format!("--bundle-root={}", bundle_alias.display()))
        .arg("--config")
        .arg(bundle_alias.join("config/root.yaml"))
        .args(["check", "--fix", "--non-interactive"]);
    let output = capture(command);

    assert_exit(&output, 0);
    assert!(output.stderr.is_empty(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "contained sibling",
        "../scripts/root.sh",
        "../../scripts/child.sh",
    ] {
        assert!(
            stdout.contains(expected),
            "missing {expected:?}: {output:?}"
        );
    }
}

#[test]
fn config_source_escapes_are_preflight_errors() {
    let (directory, bundle) = new_bundle();
    let outside = directory.path().join("outside.yaml");
    let root = bundle.join("root.yaml");
    let marker = directory.path().join("must-not-run");
    write_file(&outside, "rules:\n  - check: 'true'\n");
    let relative_include = concat!(
        "rules:\n",
        "  - check: ': > \"$RULESY_BUNDLE_ROOT_MARKER\"'\n",
        "  - remote: ../outside.yaml\n",
    );
    write_file(&root, relative_include);

    let mut command = confined_command(&bundle, &root);
    command
        .arg("--no-fail")
        .env("RULESY_BUNDLE_ROOT_MARKER", &marker);
    assert_rejected(&capture(command), &["escapes bundle root"]);
    assert!(!marker.exists(), "a rule ran before include confinement");
    assert_confined_rejected(&bundle, &outside, &["escapes bundle root"]);
}

#[test]
fn git_and_stdin_sources_are_rejected() {
    let (_directory, bundle) = new_bundle();
    let root = bundle.join("root.yaml");
    write_file(
        &root,
        "rules:\n  - remote: git+https://example.invalid/config.git\n",
    );
    assert_confined_rejected(&bundle, &root, &["git remote include", "--bundle-root"]);

    let mut command = rulesy();
    command
        .arg("--bundle-root")
        .arg(&bundle)
        .args(["--stdin-config", "check"]);
    assert_rejected(&capture(command), &["cannot be used with stdin"]);
}

#[test]
fn lexical_pattern_escapes_fail_before_rules() {
    let (directory, bundle) = new_bundle();
    let config = bundle.join("config");
    fs::create_dir_all(config.join("scripts")).unwrap();
    let root = config.join("root.yaml");
    let marker = directory.path().join("must-not-run");

    for patterns in [
        "  - ../../outside/*.sh\n",
        "  - scripts/*.sh\n  - '!   ../../outside/*.sh'\n",
        "  - '.[.]/.[.]/bundle/config/scripts/*.sh'\n",
        "  - /outside/*.sh\n",
    ] {
        write_file(
            &root,
            format!(
                "rules:\n  - check: ': > \"$RULESY_BUNDLE_ROOT_MARKER\"'\npatterns:\n{patterns}"
            ),
        );
        let mut command = confined_command(&bundle, &root);
        command.env("RULESY_BUNDLE_ROOT_MARKER", &marker);
        assert_rejected(&capture(command), &["pattern", "bundle root"]);
        assert!(!marker.exists(), "a rule ran before pattern confinement");
    }
}

#[cfg(unix)]
#[test]
fn symlinked_config_and_pattern_escapes_are_rejected() {
    use std::os::unix::fs::symlink;

    let (directory, bundle) = new_bundle();
    let config = bundle.join("config");
    let outside = directory.path().join("outside");
    fs::create_dir(&config).unwrap();
    fs::create_dir(&outside).unwrap();
    write_file(&config.join("inside.yaml"), "rules:\n  - check: 'true'\n");
    write_file(&outside.join("outside.sh"), "#!/bin/sh\nexit 0\n");
    let root = config.join("root.yaml");

    symlink("../bundle/config/inside.yaml", outside.join("return")).unwrap();
    symlink("../../outside/return", config.join("included.yaml")).unwrap();
    assert_confined_rejected(
        &bundle,
        &config.join("included.yaml"),
        &["escapes bundle root"],
    );
    write_file(&root, "rules:\n  - remote: included.yaml\n");
    assert_confined_rejected(&bundle, &root, &["escapes bundle root"]);
    assert_confined_rejected(
        &bundle,
        &config.join("../../outside/../bundle/config/inside.yaml"),
        &["escapes bundle root"],
    );

    fs::remove_file(config.join("included.yaml")).unwrap();
    fs::create_dir(config.join("scripts")).unwrap();
    write_file(&config.join("scripts/inside.sh"), "#!/bin/sh\nexit 0\n");
    symlink(&config, outside.join("back")).unwrap();
    symlink(&outside, config.join("linked")).unwrap();
    write_file(&root, "patterns:\n  - '*/back/scripts/inside.sh'\n");
    assert_confined_rejected(&bundle, &root, &["pattern prefix", "escapes bundle root"]);

    fs::create_dir(config.join("in")).unwrap();
    symlink(&outside, config.join("in/linked")).unwrap();
    write_file(&root, "patterns:\n  - '*/linked/never.sh'\n");
    assert_confined_rejected(&bundle, &root, &["pattern prefix", "escapes bundle root"]);

    symlink(
        "../../../outside/back/scripts/inside.sh",
        config.join("scripts/reentry.sh"),
    )
    .unwrap();
    write_file(&root, "patterns:\n  - scripts/reentry.sh\n");
    assert_confined_rejected(&bundle, &root, &["pattern", "escapes bundle root"]);

    symlink(
        outside.join("outside.sh"),
        config.join("scripts/outside.sh"),
    )
    .unwrap();
    write_file(&root, "patterns:\n  - scripts/*.sh\n");
    assert_confined_rejected(&bundle, &root, &["pattern match", "escapes bundle root"]);

    symlink(".", config.join("loop")).unwrap();
    write_file(&root, "patterns:\n  - '**/*.sh'\n");
    assert_confined_rejected(&bundle, &root, &["recursive pattern", "bundle root"]);
}

#[cfg(unix)]
#[test]
fn selected_scripts_are_rechecked_immediately_before_execution() {
    let (directory, bundle) = new_bundle();
    let scripts = bundle.join("scripts");
    fs::create_dir(&scripts).unwrap();
    write_file(&scripts.join("selected.sh"), "#!/bin/sh\nexit 0\n");
    let marker = directory.path().join("outside-ran");
    let outside = directory.path().join("outside.sh");
    write_file(
        &outside,
        "#!/bin/sh\n: > \"$RULESY_BUNDLE_ROOT_OUTSIDE_RAN\"\n",
    );
    let root = bundle.join("root.yaml");
    write_file(
        &root,
        concat!(
            "rules:\n  - check: |\n",
            "      rm scripts/selected.sh\n",
            "      ln -s \"$RULESY_BUNDLE_ROOT_OUTSIDE\" scripts/selected.sh\n",
            "patterns:\n  - scripts/*.sh\n",
        ),
    );

    let mut command = confined_command(&bundle, &root);
    command
        .env("RULESY_BUNDLE_ROOT_OUTSIDE", &outside)
        .env("RULESY_BUNDLE_ROOT_OUTSIDE_RAN", &marker);
    assert_rejected(
        &capture(command),
        &["pattern script", "escapes bundle root"],
    );
    assert!(!marker.exists(), "the rewired outside script executed");
}

#[cfg(unix)]
#[test]
fn confined_pattern_selection_is_frozen_before_commands_run() {
    let (_directory, bundle) = new_bundle();
    let scripts = bundle.join("scripts");
    fs::create_dir(&scripts).unwrap();
    let root = bundle.join("root.yaml");
    write_file(
        &root,
        concat!(
            "rules:\n  - name: materialize late script\n    check: |\n",
            "      printf '#!/bin/sh\\nexit 99\\n' > scripts/late.sh\n",
            "patterns:\n  - scripts/*.sh\n",
        ),
    );

    let output = capture(confined_command(&bundle, &root));
    assert_exit(&output, 0);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("materialize late script"));
    assert!(!stdout.contains("scripts/late.sh"));

    let late = scripts.join("late.sh");
    assert!(late.is_file());
    fs::remove_file(&late).unwrap();
    let mut command = rulesy();
    command.arg("--config").arg(&root).arg("check");
    let unconfined = capture(command);
    assert_exit(&unconfined, 3);
    assert!(String::from_utf8_lossy(&unconfined.stdout).contains("scripts/late.sh"));
}
