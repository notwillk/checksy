use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

mod support;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProvisioningLockCorpus {
    schema_version: u64,
    cases: Vec<ProvisioningLockCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProvisioningLockCase {
    id: String,
    fixture: String,
    assets: Vec<String>,
    modes: Vec<String>,
    test: String,
    #[serde(default)]
    expected_exit: Option<i32>,
    #[serde(default)]
    expected_holder_exit: Option<i32>,
    #[serde(default)]
    expected_contender_exit: Option<i32>,
    #[serde(default)]
    expected_no_fail_exit: Option<i32>,
    #[serde(default)]
    expected_trace: Option<Vec<String>>,
    #[serde(default)]
    expected_progress: Option<bool>,
    #[serde(default)]
    expected_cache_mutation: Option<bool>,
}

const EXECUTABLE_TESTS: &[&str] = &[
    "auto_discovered_provisioning_uses_the_per_user_lock",
    "check_only_runs_remain_lock_free",
    "configuration_and_cache_aliases_share_the_lock",
    "contention_precedes_commands_and_is_not_masked",
    "file_and_stdin_provisioning_share_one_lock",
    "invalid_configuration_precedes_lock",
    "legacy_git_acquisition_is_locked",
    "lock_releases_after_all_outcomes",
    "passing_fix_mode_still_contends",
    "stale_contents_are_ignored_and_preserved",
];

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join("provisioning-lock")
}

fn corpus() -> ProvisioningLockCorpus {
    let document = fs::read_to_string(fixture_root().join("cases.yaml")).unwrap();
    let corpus: ProvisioningLockCorpus = serde_yaml::from_str(&document).unwrap();
    assert_eq!(corpus.schema_version, 1);
    corpus
}

fn case<'a>(corpus: &'a ProvisioningLockCorpus, id: &str) -> &'a ProvisioningLockCase {
    corpus
        .cases
        .iter()
        .find(|case| case.id == id)
        .unwrap_or_else(|| panic!("provisioning-lock corpus omitted {id:?}"))
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

#[test]
fn corpus_is_closed_unique_and_mapped_to_executable_tests() {
    let corpus = corpus();
    let ids: BTreeSet<_> = corpus.cases.iter().map(|case| case.id.as_str()).collect();
    assert_eq!(ids.len(), corpus.cases.len(), "duplicate case ID");

    let known_tests: BTreeSet<_> = EXECUTABLE_TESTS.iter().copied().collect();
    let indexed_tests: BTreeSet<_> = corpus.cases.iter().map(|case| case.test.as_str()).collect();
    assert_eq!(indexed_tests, known_tests);

    for case in &corpus.cases {
        assert!(!case.modes.is_empty(), "{} has no execution mode", case.id);
        for value in [
            case.expected_exit,
            case.expected_holder_exit,
            case.expected_contender_exit,
            case.expected_no_fail_exit,
        ]
        .into_iter()
        .flatten()
        {
            assert!([0, 2, 3, 4].contains(&value), "{} has bad exit", case.id);
        }
        if let Some(trace) = &case.expected_trace {
            assert!(!trace.is_empty(), "{} has an empty trace", case.id);
        }
        let _ = (case.expected_progress, case.expected_cache_mutation);
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
        assert_eq!(bytes.last(), Some(&b'\n'), "{path} needs a final newline");
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix {
    use super::*;
    use std::ffi::CString;
    use std::fs::{File, OpenOptions};
    use std::io::{BufRead, BufReader, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, Output, Stdio};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

    fn checksy() -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_checksy"));
        command.process_group(0);
        command
    }

    fn code(output: &Output) -> i32 {
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

    fn wait_output_bounded(child: Child) -> Output {
        let pid = child.id() as libc::pid_t;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(child.wait_with_output());
        });
        match receiver.recv_timeout(COMMAND_TIMEOUT) {
            Ok(output) => output.unwrap(),
            Err(error) => {
                unsafe { libc::kill(-pid, libc::SIGTERM) };
                match receiver.recv_timeout(Duration::from_secs(3)) {
                    Ok(output) => output.unwrap(),
                    Err(_) => {
                        unsafe { libc::kill(-pid, libc::SIGKILL) };
                        match receiver.recv_timeout(Duration::from_secs(3)) {
                            Ok(output) => output.unwrap(),
                            Err(_) => panic!("checksy did not exit after watchdog: {error}"),
                        }
                    }
                }
            }
        }
    }

    fn run_bounded(mut command: Command, input: Option<&[u8]>) -> Output {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        if input.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        let mut child = command.spawn().unwrap();
        if let Some(input) = input {
            child.stdin.take().unwrap().write_all(input).unwrap();
        }
        wait_output_bounded(child)
    }

    fn file_command(root: &Path, fixture: &str, command: &str, extra: &[&str]) -> Command {
        let mut process = checksy();
        process
            .current_dir(root)
            .arg("--config")
            .arg(root.join(fixture))
            .arg(command)
            .args(extra);
        process
    }

    fn stdin_command(root: &Path, command: &str, extra: &[&str], dash: bool) -> Command {
        let mut process = checksy();
        process.current_dir(root);
        if dash {
            process.args(["--config", "-"]);
        } else {
            process.arg("--stdin-config");
        }
        process.arg(command).args(extra);
        process
    }

    fn set_trace_environment(command: &mut Command, trace: &Path, fixed: &Path) {
        command
            .env("CHECKSY_PROVISION_TRACE", trace)
            .env("CHECKSY_PROVISION_FIXED", fixed);
    }

    fn assert_lock_held(output: &Output, deprecated: bool) {
        assert_eq!(code(output), 4, "{}", output_context(output));
        assert!(output.stdout.is_empty(), "{}", output_context(output));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(
            "provisioning lock held: another checksy check --fix is already running for this user"
        ));
        assert_eq!(stderr.contains("diagnose\" is deprecated"), deprecated);
    }

    fn make_fifo(path: &Path) {
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    }

    fn wait_for_fifo_line(file: &File) {
        let mut descriptor = libc::pollfd {
            fd: file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(unsafe { libc::poll(&mut descriptor, 1, 10_000) }, 1);
        let mut line = String::new();
        BufReader::new(file).read_line(&mut line).unwrap();
        assert_eq!(line, "ready\n");
    }

    struct Holder {
        child: Option<Child>,
        release: File,
    }

    impl Holder {
        fn spawn(root: &Path, label: &str, stdin: bool) -> Self {
            let ready_path = root.join(format!(".{label}-ready.fifo"));
            let release_path = root.join(format!(".{label}-release.fifo"));
            make_fifo(&ready_path);
            make_fifo(&release_path);
            let ready = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&ready_path)
                .unwrap();
            let release = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&release_path)
                .unwrap();
            let trace = root.join(format!(".{label}-trace"));
            let fixed = root.join(format!(".{label}-fixed"));
            let mut command = if stdin {
                stdin_command(root, "check", &["--fix", "--non-interactive"], false)
            } else {
                file_command(
                    root,
                    "blocking-fix.yaml",
                    "check",
                    &["--fix", "--non-interactive"],
                )
            };
            set_trace_environment(&mut command, &trace, &fixed);
            command
                .env("CHECKSY_PROVISION_READY_FIFO", &ready_path)
                .env("CHECKSY_PROVISION_RELEASE_FIFO", &release_path)
                .env("HOME", root.join(".ignored-home"))
                .env("XDG_STATE_HOME", root.join(".ignored-xdg-state"))
                .stdin(if stdin { Stdio::piped() } else { Stdio::null() })
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let mut child = command.spawn().unwrap();
            if stdin {
                let document = fs::read(root.join("blocking-fix.yaml")).unwrap();
                child.stdin.take().unwrap().write_all(&document).unwrap();
            }
            wait_for_fifo_line(&ready);
            Self {
                child: Some(child),
                release,
            }
        }

        fn finish(mut self) -> Output {
            writeln!(self.release, "release").unwrap();
            self.release.flush().unwrap();
            wait_output_bounded(self.child.take().unwrap())
        }
    }

    impl Drop for Holder {
        fn drop(&mut self) {
            let _ = writeln!(self.release, "release");
            let _ = self.release.flush();
            let Some(child) = self.child.as_mut() else {
                return;
            };
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) if Instant::now() < deadline => {
                        let _ = rustix::io::poll(&mut [], 25);
                    }
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return;
                    }
                }
            }
        }
    }

    fn writable_fixture_copy() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        copy_directory(&fixture_root(), directory.path());
        directory
    }

    #[test]
    fn file_and_stdin_provisioning_share_one_lock() {
        let _serial = support::provisioning_test_guard();
        let corpus = corpus();
        let indexed = case(&corpus, "file-and-stdin-fix");
        let copy = writable_fixture_copy();

        for (index, mode) in ["file", "stdin-config", "config-dash"]
            .into_iter()
            .enumerate()
        {
            let trace = copy.path().join(format!(".{mode}-trace"));
            let fixed = copy.path().join(format!(".{mode}-fixed"));
            let document = fs::read(copy.path().join(&indexed.fixture)).unwrap();
            let mut command = match mode {
                "file" => file_command(
                    copy.path(),
                    &indexed.fixture,
                    "check",
                    &["--fix", "--non-interactive"],
                ),
                "stdin-config" => {
                    stdin_command(copy.path(), "check", &["--fix", "--non-interactive"], false)
                }
                "config-dash" => {
                    stdin_command(copy.path(), "check", &["--fix", "--non-interactive"], true)
                }
                _ => unreachable!(),
            };
            set_trace_environment(&mut command, &trace, &fixed);
            let output = run_bounded(command, (index != 0).then_some(document.as_slice()));
            assert_eq!(
                code(&output),
                indexed.expected_exit.unwrap(),
                "{}",
                output_context(&output)
            );
            assert_eq!(
                fs::read_to_string(trace).unwrap(),
                indexed.expected_trace.as_ref().unwrap().join("\n") + "\n"
            );
        }
    }

    #[test]
    fn contention_precedes_commands_and_is_not_masked() {
        let _serial = support::provisioning_test_guard();
        let copy = writable_fixture_copy();
        let holder = Holder::spawn(copy.path(), "file-holder", false);

        for (index, (command_name, stdin, extra)) in [
            ("check", false, vec!["--fix"]),
            ("check", false, vec!["--fix", "--no-fail"]),
            ("check", true, vec!["--fix"]),
            ("diagnose", false, vec!["--fix", "--non-interactive"]),
        ]
        .into_iter()
        .enumerate()
        {
            let trace = copy.path().join(format!(".loser-{index}-trace"));
            let fixed = copy.path().join(format!(".loser-{index}-fixed"));
            let document = fs::read(copy.path().join("passing.yaml")).unwrap();
            let mut command = if stdin {
                stdin_command(copy.path(), command_name, &extra, false)
            } else {
                file_command(copy.path(), "passing.yaml", command_name, &extra)
            };
            set_trace_environment(&mut command, &trace, &fixed);
            let output = run_bounded(command, stdin.then_some(document.as_slice()));
            assert_lock_held(&output, command_name == "diagnose");
            assert!(!trace.exists(), "contender executed a configured command");
        }

        let holder_output = holder.finish();
        assert_eq!(
            code(&holder_output),
            0,
            "{}",
            output_context(&holder_output)
        );
    }

    #[test]
    fn passing_fix_mode_still_contends() {
        let _serial = support::provisioning_test_guard();
        let copy = writable_fixture_copy();
        let holder = Holder::spawn(copy.path(), "passing-contender", false);
        let trace = copy.path().join(".passing-loser-trace");
        let fixed = copy.path().join(".passing-loser-fixed");
        let mut command = file_command(copy.path(), "passing.yaml", "check", &["--fix"]);
        set_trace_environment(&mut command, &trace, &fixed);
        let output = run_bounded(command, None);
        assert_lock_held(&output, false);
        assert!(!trace.exists());
        let holder_output = holder.finish();
        assert_eq!(
            code(&holder_output),
            0,
            "{}",
            output_context(&holder_output)
        );
    }

    #[test]
    fn check_only_runs_remain_lock_free() {
        let _serial = support::provisioning_test_guard();
        let copy = writable_fixture_copy();
        let holder = Holder::spawn(copy.path(), "check-only", false);
        let document = fs::read(copy.path().join("passing.yaml")).unwrap();

        for (index, stdin) in [false, true].into_iter().enumerate() {
            let trace = copy.path().join(format!(".check-only-{index}-trace"));
            let fixed = copy.path().join(format!(".check-only-{index}-fixed"));
            let mut command = if stdin {
                stdin_command(copy.path(), "check", &[], false)
            } else {
                file_command(copy.path(), "passing.yaml", "check", &[])
            };
            set_trace_environment(&mut command, &trace, &fixed);
            let output = run_bounded(command, stdin.then_some(document.as_slice()));
            assert_eq!(code(&output), 0, "{}", output_context(&output));
            assert_eq!(fs::read_to_string(trace).unwrap(), "pass\n");
        }

        let install = file_command(copy.path(), "passing.yaml", "install", &[]);
        let output = run_bounded(install, None);
        assert_eq!(code(&output), 0, "{}", output_context(&output));
        let holder_output = holder.finish();
        assert_eq!(
            code(&holder_output),
            0,
            "{}",
            output_context(&holder_output)
        );
    }

    #[test]
    fn invalid_configuration_precedes_lock() {
        let _serial = support::provisioning_test_guard();
        let copy = writable_fixture_copy();
        let holder = Holder::spawn(copy.path(), "invalid", false);
        let document = fs::read(copy.path().join("invalid-no-execution.yaml")).unwrap();

        for (index, stdin) in [false, true].into_iter().enumerate() {
            let trace = copy.path().join(format!(".invalid-{index}-trace"));
            let fixed = copy.path().join(format!(".invalid-{index}-fixed"));
            let mut command = if stdin {
                stdin_command(copy.path(), "check", &["--fix"], false)
            } else {
                file_command(
                    copy.path(),
                    "invalid-no-execution.yaml",
                    "check",
                    &["--fix"],
                )
            };
            set_trace_environment(&mut command, &trace, &fixed);
            let output = run_bounded(command, stdin.then_some(document.as_slice()));
            assert_eq!(code(&output), 2, "{}", output_context(&output));
            assert!(!String::from_utf8_lossy(&output.stderr).contains("provisioning lock held"));
            assert!(!trace.exists());
        }

        let trace = copy.path().join(".invalid-invocation-trace");
        let fixed = copy.path().join(".invalid-invocation-fixed");
        let mut command = file_command(
            copy.path(),
            "passing.yaml",
            "check",
            &["--fix", "--unknown-provisioning-flag"],
        );
        set_trace_environment(&mut command, &trace, &fixed);
        let output = run_bounded(command, None);
        assert_eq!(code(&output), 2, "{}", output_context(&output));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("provisioning lock held"));
        assert!(!trace.exists());

        let holder_output = holder.finish();
        assert_eq!(
            code(&holder_output),
            0,
            "{}",
            output_context(&holder_output)
        );
    }

    #[test]
    fn configuration_and_cache_aliases_share_the_lock() {
        let _serial = support::provisioning_test_guard();
        let copy = writable_fixture_copy();
        let holder = Holder::spawn(copy.path(), "aliases", true);
        assert!(!copy.path().join(".ignored-home").exists());
        assert!(!copy.path().join(".ignored-xdg-state").exists());
        let alias = copy.path().join("passing-alias.yaml");
        symlink(copy.path().join("passing.yaml"), &alias).unwrap();

        for (index, fixture) in ["passing-alias.yaml", "alternate-cache-path.yaml"]
            .into_iter()
            .enumerate()
        {
            let trace = copy.path().join(format!(".alias-{index}-trace"));
            let fixed = copy.path().join(format!(".alias-{index}-fixed"));
            let mut command = file_command(copy.path(), fixture, "check", &["--fix"]);
            set_trace_environment(&mut command, &trace, &fixed);
            let output = run_bounded(command, None);
            assert_lock_held(&output, false);
            assert!(!trace.exists());
        }

        let holder_output = holder.finish();
        assert_eq!(
            code(&holder_output),
            0,
            "{}",
            output_context(&holder_output)
        );
    }

    #[test]
    fn legacy_git_acquisition_is_locked() {
        let _serial = support::provisioning_test_guard();
        let copy = writable_fixture_copy();
        let holder = Holder::spawn(copy.path(), "legacy-git", false);
        let cache = copy.path().join(".provisioning-lock-legacy-cache");
        let output = run_bounded(
            file_command(
                copy.path(),
                "legacy-missing-git.yaml",
                "check",
                &["--fix", "--no-fail"],
            ),
            None,
        );
        assert_lock_held(&output, false);
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Caching"));
        assert!(!cache.exists(), "legacy cache mutated before contention");
        let holder_output = holder.finish();
        assert_eq!(
            code(&holder_output),
            0,
            "{}",
            output_context(&holder_output)
        );

        let repository = copy.path().join("local-legacy-remote");
        fs::create_dir(&repository).unwrap();
        for arguments in [
            vec!["init", "--initial-branch", "main"],
            vec!["config", "user.name", "Checksy Fixture"],
            vec!["config", "user.email", "checksy-fixture@example.invalid"],
        ] {
            let status = Command::new("git")
                .current_dir(&repository)
                .args(arguments)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
        }
        fs::write(
            repository.join(".checksy.yaml"),
            "rules:\n  - name: cloned local definition\n    check: 'true'\n",
        )
        .unwrap();
        for arguments in [
            vec!["add", ".checksy.yaml"],
            vec!["commit", "-m", "fixture"],
        ] {
            let status = Command::new("git")
                .current_dir(&repository)
                .args(arguments)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
        }

        let successful_config = copy.path().join("successful-legacy-git.yaml");
        fs::write(
            &successful_config,
            format!(
                "cachePath: .successful-legacy-cache\nrules:\n  - remote: 'git+{}#main:.checksy.yaml'\n",
                repository.display()
            ),
        )
        .unwrap();

        let wrapper_directory = copy.path().join("git-wrapper");
        fs::create_dir(&wrapper_directory).unwrap();
        let wrapper = wrapper_directory.join("git");
        fs::copy(copy.path().join("scripts/blocking-git.sh"), &wrapper).unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        let real_git = Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .unwrap();
        assert!(real_git.status.success());
        let real_git = String::from_utf8(real_git.stdout).unwrap();
        let real_git = real_git.trim();

        let ready_path = copy.path().join(".git-ready.fifo");
        let release_path = copy.path().join(".git-release.fifo");
        make_fifo(&ready_path);
        make_fifo(&release_path);
        let ready = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&ready_path)
            .unwrap();
        let release = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&release_path)
            .unwrap();
        let mut search_path = vec![wrapper_directory.clone()];
        search_path.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let path = std::env::join_paths(search_path).unwrap();
        let mut command = file_command(
            copy.path(),
            "successful-legacy-git.yaml",
            "check",
            &["--fix", "--non-interactive"],
        );
        command
            .env("PATH", path)
            .env("CHECKSY_REAL_GIT", real_git)
            .env("CHECKSY_PROVISION_GIT_READY_FIFO", &ready_path)
            .env("CHECKSY_PROVISION_GIT_RELEASE_FIFO", &release_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = command.spawn().unwrap();
        let mut local_git_holder = Holder {
            child: Some(child),
            release,
        };
        wait_for_fifo_line(&ready);

        let trace = copy.path().join(".during-git-trace");
        let fixed = copy.path().join(".during-git-fixed");
        let mut contender = file_command(copy.path(), "passing.yaml", "check", &["--fix"]);
        set_trace_environment(&mut contender, &trace, &fixed);
        let output = run_bounded(contender, None);
        assert_lock_held(&output, false);
        assert!(!trace.exists());

        writeln!(local_git_holder.release, "release").unwrap();
        local_git_holder.release.flush().unwrap();
        let output = wait_output_bounded(local_git_holder.child.take().unwrap());
        assert_eq!(code(&output), 0, "{}", output_context(&output));
        assert!(String::from_utf8_lossy(&output.stdout).contains("Caching missing git remotes"));
        assert!(copy.path().join(".successful-legacy-cache").exists());
    }

    #[test]
    fn auto_discovered_provisioning_uses_the_per_user_lock() {
        let _serial = support::provisioning_test_guard();
        let copy = writable_fixture_copy();
        let holder = Holder::spawn(copy.path(), "autodiscovery", false);
        let trace = copy.path().join(".autodiscovery-contender-trace");
        let fixed = copy.path().join(".autodiscovery-contender-fixed");
        let mut contender = checksy();
        contender
            .current_dir(copy.path().join("autodiscovery"))
            .args(["check", "--fix", "--non-interactive"]);
        set_trace_environment(&mut contender, &trace, &fixed);
        let output = run_bounded(contender, None);
        assert_lock_held(&output, false);
        assert!(!trace.exists());
        let holder_output = holder.finish();
        assert_eq!(
            code(&holder_output),
            0,
            "{}",
            output_context(&holder_output)
        );

        let mut command = checksy();
        command
            .current_dir(copy.path().join("autodiscovery"))
            .args(["check", "--fix", "--non-interactive"]);
        set_trace_environment(&mut command, &trace, &fixed);
        let output = run_bounded(command, None);
        assert_eq!(code(&output), 0, "{}", output_context(&output));
        assert_eq!(fs::read_to_string(trace).unwrap(), "check\nfix\ncheck\n");
    }

    #[test]
    fn lock_releases_after_all_outcomes() {
        let _serial = support::provisioning_test_guard();
        let copy = writable_fixture_copy();

        for (index, (fixture, extra, expected)) in [
            ("failing-fix.yaml", vec!["--fix"], 3),
            ("failing-fix.yaml", vec!["--fix", "--no-fail"], 0),
        ]
        .into_iter()
        .enumerate()
        {
            let trace = copy.path().join(format!(".outcome-{index}-trace"));
            let fixed = copy.path().join(format!(".outcome-{index}-fixed"));
            let mut command = file_command(copy.path(), fixture, "check", &extra);
            set_trace_environment(&mut command, &trace, &fixed);
            let output = run_bounded(command, None);
            assert_eq!(code(&output), expected, "{}", output_context(&output));

            let passing_trace = copy.path().join(format!(".after-{index}-trace"));
            let passing_fixed = copy.path().join(format!(".after-{index}-fixed"));
            let mut passing = file_command(copy.path(), "passing.yaml", "check", &["--fix"]);
            set_trace_environment(&mut passing, &passing_trace, &passing_fixed);
            let output = run_bounded(passing, None);
            assert_eq!(code(&output), 0, "{}", output_context(&output));
        }

        let operational = copy.path().join("operational-error.yaml");
        fs::write(
            &operational,
            "rules:\n  - name: signal failure\n    check: 'kill -TERM $$'\n",
        )
        .unwrap();
        let output = run_bounded(
            file_command(copy.path(), "operational-error.yaml", "check", &["--fix"]),
            None,
        );
        assert_eq!(code(&output), 2, "{}", output_context(&output));

        let trace = copy.path().join(".after-operational-trace");
        let fixed = copy.path().join(".after-operational-fixed");
        let mut passing = file_command(copy.path(), "passing.yaml", "check", &["--fix"]);
        set_trace_environment(&mut passing, &trace, &fixed);
        let output = run_bounded(passing, None);
        assert_eq!(code(&output), 0, "{}", output_context(&output));
    }
}
