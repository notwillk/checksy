use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::fd::{AsRawFd, BorrowedFd};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::unix::ffi::OsStrExt;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::unix::net::UnixDatagram;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::unix::process::{CommandExt, ExitStatusExt};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::sync::atomic::{AtomicI32, Ordering};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::sync::{mpsc, Arc};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::thread;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProcessRunnerCorpus {
    schema_version: u64,
    cases: Vec<ProcessRunnerCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProcessRunnerCase {
    id: String,
    fixture: String,
    assets: Vec<String>,
    test: String,
    expected_exit: i32,
}

const EXECUTABLE_TESTS: &[&str] = &[
    "non_interactive_command_classes_receive_eof",
    "ordinary_nonzero_remains_a_compliance_result",
    "operational_failures_are_exit_two_even_when_no_fail_is_set",
    "fix_timeout_prevents_recheck_and_later_commands",
    "leader_exit_waits_for_lingering_managed_descendant",
    "maximum_plus_one_output_is_head_tail_bounded",
    "invalid_timeout_executes_no_command",
    "ctrl_c_cleans_the_managed_tree_and_resignals_checksy",
    "runner_detaches_from_a_real_controlling_tty",
];

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join("process-runner")
}

fn corpus() -> ProcessRunnerCorpus {
    let data = fs::read_to_string(fixture_root().join("cases.yaml")).unwrap();
    let corpus: ProcessRunnerCorpus = serde_yaml::from_str(&data).unwrap();
    assert_eq!(corpus.schema_version, 1);
    corpus
}

fn case<'a>(corpus: &'a ProcessRunnerCorpus, id: &str) -> &'a ProcessRunnerCase {
    corpus
        .cases
        .iter()
        .find(|case| case.id == id)
        .unwrap_or_else(|| panic!("process-runner corpus omitted {id:?}"))
}

fn checksy() -> Command {
    Command::new(env!("CARGO_BIN_EXE_checksy"))
}

fn config_args(path: &Path, extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        "--config".to_string(),
        path.to_string_lossy().into_owned(),
        "check".to_string(),
    ];
    args.extend(extra.iter().map(|value| (*value).to_string()));
    args
}

fn run_config(path: &Path, extra: &[&str]) -> Output {
    checksy().args(config_args(path, extra)).output().unwrap()
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

fn assert_exit(case: &ProcessRunnerCase, output: &Output) {
    assert_eq!(
        exit_code(output),
        case.expected_exit,
        "{}: stdout={:?} stderr={:?}",
        case.id,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
            fs::copy(&source_path, &destination_path).unwrap();
        }
    }
}

fn writable_fixture_copy() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    copy_directory(&fixture_root(), directory.path());
    directory
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

#[test]
fn corpus_is_closed_unique_and_mapped_to_executable_tests() {
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
        "a configuration is assigned to more than one case"
    );

    let known_tests: BTreeSet<_> = EXECUTABLE_TESTS.iter().copied().collect();
    let indexed_tests: BTreeSet<_> = corpus.cases.iter().map(|case| case.test.as_str()).collect();
    assert_eq!(indexed_tests, known_tests);

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
        assert!(!bytes.contains(&b'\r'), "{path} must use LF line endings");
        assert_eq!(
            bytes.last(),
            Some(&b'\n'),
            "{path} needs a terminal newline"
        );
    }
}

#[test]
fn non_interactive_command_classes_receive_eof() {
    let corpus = corpus();

    let check_copy = writable_fixture_copy();
    let check_case = case(&corpus, "check-stdin-eof");
    let output = run_config(&check_copy.path().join(&check_case.fixture), &[]);
    assert_exit(check_case, &output);
    assert!(check_copy.path().join(".check-eof").is_file());

    let fix_copy = writable_fixture_copy();
    let fix_case = case(&corpus, "fix-recheck-stdin-eof");
    let output = run_config(&fix_copy.path().join(&fix_case.fixture), &["--fix"]);
    assert_exit(fix_case, &output);
    for marker in [
        ".initial-check-eof",
        ".fix-eof",
        ".fixed",
        ".final-recheck-eof",
    ] {
        assert!(fix_copy.path().join(marker).is_file(), "missing {marker}");
    }

    let pattern_copy = writable_fixture_copy();
    let pattern_case = case(&corpus, "pattern-only-stdin-eof");
    let output = run_config(&pattern_copy.path().join(&pattern_case.fixture), &[]);
    assert_exit(pattern_case, &output);
    assert!(pattern_copy.path().join(".pattern-eof").is_file());
}

#[test]
fn ordinary_nonzero_remains_a_compliance_result() {
    let corpus = corpus();
    let case = case(&corpus, "ordinary-nonzero");
    let copy = writable_fixture_copy();
    let config = copy.path().join(&case.fixture);

    let output = run_config(&config, &[]);
    assert_exit(case, &output);
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostics.contains("ordinary stdout sentinel"));
    assert!(diagnostics.contains("ordinary stderr sentinel"));

    let output = run_config(&config, &["--no-fail"]);
    assert_eq!(exit_code(&output), 0);
    assert!(String::from_utf8_lossy(&output.stdout).contains("rules failed validation"));
}

#[test]
fn operational_failures_are_exit_two_even_when_no_fail_is_set() {
    let corpus = corpus();

    let spawn_case = case(&corpus, "spawn-failure");
    let spawn_copy = writable_fixture_copy();
    let output = checksy()
        .args(config_args(
            &spawn_copy.path().join(&spawn_case.fixture),
            &["--no-fail"],
        ))
        .env("PATH", "")
        .output()
        .unwrap();
    assert_exit(spawn_case, &output);
    assert!(!output.stderr.is_empty());

    let timeout_case = case(&corpus, "timeout-partial-output");
    let timeout_copy = writable_fixture_copy();
    let output = run_config(
        &timeout_copy.path().join(&timeout_case.fixture),
        &["--no-fail"],
    );
    assert_exit(timeout_case, &output);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("stdout before timeout"));
    assert!(combined.contains("stderr before timeout"));
    assert!(combined.to_ascii_lowercase().contains("timed out"));
    assert!(!timeout_copy.path().join(".unexpected-timeout-fix").exists());
    assert!(!timeout_copy
        .path()
        .join(".unexpected-after-timeout")
        .exists());

    let signal_case = case(&corpus, "child-signal");
    let signal_copy = writable_fixture_copy();
    let output = run_config(
        &signal_copy.path().join(&signal_case.fixture),
        &["--no-fail"],
    );
    assert_exit(signal_case, &output);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("stdout before child signal"));
    assert!(combined.contains("stderr before child signal"));
    assert!(combined.to_ascii_lowercase().contains("signal"));
}

#[test]
fn fix_timeout_prevents_recheck_and_later_commands() {
    let corpus = corpus();
    let case = case(&corpus, "fix-timeout-fail-fast");
    let copy = writable_fixture_copy();
    let output = run_config(&copy.path().join(&case.fixture), &["--fix", "--no-fail"]);
    assert_exit(case, &output);
    assert!(copy.path().join(".fix-started").is_file());
    assert!(!copy.path().join(".unexpected-recheck").exists());
    assert!(!copy.path().join(".unexpected-later-command").exists());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("fix stdout before timeout"));
    assert!(combined.contains("fix stderr before timeout"));
}

#[test]
fn maximum_plus_one_output_is_head_tail_bounded() {
    let corpus = corpus();
    let case = case(&corpus, "maximum-plus-one-output");
    let copy = writable_fixture_copy();
    let output = run_config(&copy.path().join(&case.fixture), &[]);
    assert_exit(case, &output);
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostics.contains("... 1 bytes omitted from bounded process output ..."));
    assert!(diagnostics.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    assert!(diagnostics.contains('Z'));
    assert!(output.stderr.len() < 1_100_000);
}

#[test]
fn invalid_timeout_executes_no_command() {
    let corpus = corpus();
    let case = case(&corpus, "invalid-timeout-no-execution");
    let copy = writable_fixture_copy();
    let output = run_config(&copy.path().join(&case.fixture), &["--no-fail"]);
    assert_exit(case, &output);
    assert!(!copy.path().join(".invalid-timeout-ran").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("timeout"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn helper_command(role: &str, directory: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--ignored", "--exact", "process_tree_helper", "--nocapture"])
        .env("CHECKSY_HELPER_ROLE", role)
        .env("CHECKSY_HELPER_DIR", directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn wait_for_helper_ready(child: &mut std::process::Child) {
    let stdout = child.stdout.take().expect("helper stdout pipe");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).expect("helper readiness read");
        assert_ne!(bytes, 0, "helper exited before readiness");
        if line.trim() == "CHECKSY_HELPER_READY" {
            break;
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn hold_advisory_lock(path: &Path) -> fs::File {
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive).unwrap();
    file
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn assert_lock_reacquirable(path: &Path) {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .unwrap_or_else(|error| panic!("{} remained locked: {error}", path.display()));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn pid_from_raw(raw: i32) -> Option<rustix::process::Pid> {
    // SAFETY: callers supply positive PIDs received directly from spawned children.
    #[cfg(target_os = "linux")]
    unsafe {
        rustix::process::Pid::from_raw(raw.try_into().ok()?)
    }
    #[cfg(target_os = "macos")]
    unsafe {
        rustix::process::Pid::from_raw(raw)
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn kill_if_present(pid: rustix::process::Pid, signal: rustix::process::Signal) {
    match rustix::process::kill_process(pid, signal) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => {}
        Err(error) => panic!("failed to signal {pid:?}: {error}"),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn kill_group_if_present(pid: rustix::process::Pid) {
    match rustix::process::kill_process_group(pid, rustix::process::Signal::Kill) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => {}
        Err(error) => panic!("failed to kill helper group {pid:?}: {error}"),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
#[ignore = "isolated subprocess workload; invoked by the process-runner contract test"]
fn process_tree_helper() {
    let role = std::env::var("CHECKSY_HELPER_ROLE").expect("helper role");
    let directory = PathBuf::from(std::env::var_os("CHECKSY_HELPER_DIR").expect("helper dir"));

    match role.as_str() {
        "grandchild" => {
            let _lock = hold_advisory_lock(&directory.join("grandchild.lock"));
            println!("CHECKSY_HELPER_READY");
            std::io::stdout().flush().unwrap();
            loop {
                thread::park();
            }
        }
        "lingering" => {
            let _lock = hold_advisory_lock(&directory.join("lingering.lock"));
            fs::write(
                directory.join("lingering.pid"),
                std::process::id().to_string(),
            )
            .unwrap();
            fs::write(directory.join("lingering.ready"), b"ready\n").unwrap();
            loop {
                thread::park();
            }
        }
        "child" => {
            let _lock = hold_advisory_lock(&directory.join("child.lock"));
            let mut grandchild = helper_command("grandchild", &directory).spawn().unwrap();
            wait_for_helper_ready(&mut grandchild);
            println!("CHECKSY_HELPER_READY");
            std::io::stdout().flush().unwrap();
            let _grandchild = grandchild;
            loop {
                thread::park();
            }
        }
        "leader" => {
            let leader = rustix::process::getpid();
            let (watchdog_cancel, watchdog_wait) = mpsc::channel::<()>();
            thread::spawn(move || {
                if watchdog_wait.recv_timeout(Duration::from_secs(8)).is_err() {
                    kill_group_if_present(leader);
                }
            });

            let mut child = helper_command("child", &directory).spawn().unwrap();
            wait_for_helper_ready(&mut child);

            let ready_path =
                PathBuf::from(std::env::var_os("CHECKSY_READY_SOCKET").expect("readiness socket"));
            let socket = UnixDatagram::unbound().unwrap();
            socket
                .send_to(
                    format!("ready:{}", leader.as_raw_nonzero()).as_bytes(),
                    ready_path,
                )
                .unwrap();

            let _watchdog_cancel = watchdog_cancel;
            let _child = child;
            loop {
                thread::park();
            }
        }
        other => panic!("unknown helper role {other:?}"),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct LingeringProcessCleanup(PathBuf);

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Drop for LingeringProcessCleanup {
    fn drop(&mut self) {
        let Ok(raw) = fs::read_to_string(self.0.join("lingering.pid")) else {
            return;
        };
        let Ok(raw) = raw.trim().parse::<i32>() else {
            return;
        };
        if let Some(pid) = pid_from_raw(raw) {
            kill_if_present(pid, rustix::process::Signal::Kill);
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn leader_exit_waits_for_lingering_managed_descendant() {
    let corpus = corpus();
    let case = case(&corpus, "leader-exit-lingering-descendant");
    let copy = writable_fixture_copy();
    let _cleanup = LingeringProcessCleanup(copy.path().to_path_buf());
    let output = checksy()
        .args(config_args(&copy.path().join(&case.fixture), &[]))
        .env("CHECKSY_PROCESS_HELPER", std::env::current_exe().unwrap())
        .env("CHECKSY_HELPER_ROLE", "lingering")
        .env("CHECKSY_HELPER_DIR", copy.path())
        .output()
        .unwrap();

    assert_exit(case, &output);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("timed out"),
        "stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_lock_reacquirable(&copy.path().join("lingering.lock"));
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[test]
fn leader_exit_waits_for_lingering_managed_descendant() {
    assert_eq!(
        case(&corpus(), "leader-exit-lingering-descendant").expected_exit,
        2
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn ctrl_c_cleans_the_managed_tree_and_resignals_checksy() {
    let corpus = corpus();
    let case = case(&corpus, "parent-interrupt-process-tree");
    let copy = writable_fixture_copy();
    let ready_path = copy.path().join("ready.sock");
    let ready = UnixDatagram::bind(&ready_path).unwrap();
    ready
        .set_read_timeout(Some(Duration::from_secs(7)))
        .unwrap();

    // A test harness may itself install signal handlers. Enter through a
    // fresh shell so caught dispositions are reset before Checksy starts,
    // matching an ordinary interactive shell invocation.
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "exec \"$CHECKSY_TEST_BIN\" --config \"$CHECKSY_TEST_CONFIG\" check",
        ])
        .env("CHECKSY_TEST_BIN", env!("CARGO_BIN_EXE_checksy"))
        .env("CHECKSY_TEST_CONFIG", copy.path().join(&case.fixture))
        .env("CHECKSY_PROCESS_HELPER", std::env::current_exe().unwrap())
        .env("CHECKSY_HELPER_DIR", copy.path())
        .env("CHECKSY_READY_SOCKET", &ready_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let checksy_pid = rustix::process::Pid::from_child(&child);

    let helper_pid_for_watchdog = Arc::new(AtomicI32::new(0));
    let watchdog_helper_pid = Arc::clone(&helper_pid_for_watchdog);
    let (watchdog_cancel, watchdog_wait) = mpsc::channel::<()>();
    let watchdog = thread::spawn(move || {
        if watchdog_wait.recv_timeout(Duration::from_secs(15)).is_err() {
            let raw = watchdog_helper_pid.load(Ordering::SeqCst);
            if let Some(pid) = pid_from_raw(raw) {
                kill_group_if_present(pid);
            }
            kill_if_present(checksy_pid, rustix::process::Signal::Kill);
        }
    });

    let mut message = [0_u8; 64];
    let bytes = match ready.recv(&mut message) {
        Ok(bytes) => bytes,
        Err(error) => {
            kill_if_present(checksy_pid, rustix::process::Signal::Kill);
            let _ = child.wait();
            let _ = watchdog_cancel.send(());
            watchdog.join().unwrap();
            panic!("managed tree did not become ready: {error}");
        }
    };
    let message = std::str::from_utf8(&message[..bytes]).unwrap();
    let helper_raw = message
        .strip_prefix("ready:")
        .unwrap()
        .parse::<i32>()
        .unwrap();
    helper_pid_for_watchdog.store(helper_raw, Ordering::SeqCst);

    let interrupted_at = Instant::now();
    kill_if_present(checksy_pid, rustix::process::Signal::Int);
    let output = child.wait_with_output().unwrap();
    let interrupt_cleanup = interrupted_at.elapsed();
    let _ = watchdog_cancel.send(());
    watchdog.join().unwrap();

    assert_eq!(
        output.status.signal(),
        Some(rustix::process::Signal::Int as i32),
        "expected SIGINT: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        interrupt_cleanup < Duration::from_secs(7),
        "the 8s inner watchdog, rather than the runner's 5s grace, cleaned the tree: {interrupt_cleanup:?}"
    );
    assert_lock_reacquirable(&copy.path().join("child.lock"));
    assert_lock_reacquirable(&copy.path().join("grandchild.lock"));
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[test]
fn ctrl_c_cleans_the_managed_tree_and_resignals_checksy() {
    let corpus = corpus();
    assert_eq!(
        case(&corpus, "parent-interrupt-process-tree").expected_exit,
        130
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn runner_detaches_from_a_real_controlling_tty() {
    let corpus = corpus();
    let case = case(&corpus, "controlling-tty-isolation");
    let copy = writable_fixture_copy();

    let master =
        rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
            .unwrap();
    rustix::pty::grantpt(&master).unwrap();
    rustix::pty::unlockpt(&master).unwrap();
    let slave_name = rustix::pty::ptsname(&master, Vec::new()).unwrap();
    let slave_path = Path::new(std::ffi::OsStr::from_bytes(slave_name.to_bytes()));
    let slave = rustix::fs::openat(
        rustix::fs::cwd(),
        slave_path,
        rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NOCTTY,
        rustix::fs::Mode::empty(),
    )
    .unwrap();
    let slave_raw = slave.as_raw_fd();

    let mut command = checksy();
    command
        .args(config_args(&copy.path().join(&case.fixture), &[]))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: the closure performs only async-signal-safe descriptor/session syscalls.
    unsafe {
        command.pre_exec(move || {
            rustix::process::setsid().map_err(std::io::Error::from)?;
            let borrowed = BorrowedFd::borrow_raw(slave_raw);
            rustix::process::ioctl_tiocsctty(borrowed).map_err(std::io::Error::from)
        });
    }

    let output = command.output().unwrap();
    drop(slave);
    drop(master);
    assert_exit(case, &output);
    assert!(
        copy.path().join(".tty-isolated").is_file(),
        "stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[test]
fn runner_detaches_from_a_real_controlling_tty() {
    let corpus = corpus();
    assert_eq!(case(&corpus, "controlling-tty-isolation").expected_exit, 0);
}
