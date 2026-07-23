use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::fs::File;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::fd::AsRawFd;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::unix::ffi::OsStrExt;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::unix::net::UnixDatagram;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::unix::process::{CommandExt, ExitStatusExt};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::{Command, ExitStatus, Output, Stdio};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::sync::mpsc;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::thread;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::time::{Duration, Instant};

mod support;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InteractiveFixCorpus {
    schema_version: u64,
    cases: Vec<InteractiveFixCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InteractiveFixCase {
    id: String,
    fixture: String,
    assets: Vec<String>,
    mode: String,
    test: String,
    expected_exit: i32,
}

const EXECUTABLE_TESTS: &[&str] = &[
    "interactive_fix_reads_stdin_and_dev_tty_then_rechecks",
    "interactive_fix_inherits_only_standard_pty_descriptors",
    "passing_interactive_fix_never_probes_for_terminal",
    "unavailable_interactive_fix_is_a_compliance_result",
    "check_only_ignores_interactive_fix",
    "deprecated_diagnose_accepts_non_interactive",
    "stdin_configuration_never_opens_a_terminal",
    "stdin_diagnostic_takes_precedence",
    "ordinary_fix_runs_under_non_interactive",
    "failed_interactive_fix_continues_and_honors_no_fail",
    "interactive_fix_timeout_is_operational_and_fail_fast",
    "ctrl_c_cleans_interactive_fix_tree_and_resignals_checksy",
    "outer_job_control_cleans_the_tree_without_suspending_checksy",
    "foreground_loss_stops_outer_relay_and_cleans_the_repair",
    "interactive_job_control_suspension_is_operational",
    "interactive_fix_streams_output_and_restores_terminal",
    "interactive_fix_receives_outer_terminal_resize",
    "precondition_interactive_fix_uses_the_same_workflow",
];

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join("interactive-fix")
}

fn corpus() -> InteractiveFixCorpus {
    let data = fs::read_to_string(fixture_root().join("cases.yaml")).unwrap();
    let corpus: InteractiveFixCorpus = serde_yaml::from_str(&data).unwrap();
    assert_eq!(corpus.schema_version, 1);
    corpus
}

fn case<'a>(corpus: &'a InteractiveFixCorpus, id: &str) -> &'a InteractiveFixCase {
    corpus
        .cases
        .iter()
        .find(|case| case.id == id)
        .unwrap_or_else(|| panic!("interactive-fix corpus omitted {id:?}"))
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

#[test]
fn corpus_is_closed_unique_and_mapped_to_executable_tests() {
    let corpus = corpus();
    let ids: BTreeSet<_> = corpus.cases.iter().map(|case| case.id.as_str()).collect();
    assert_eq!(ids.len(), corpus.cases.len(), "duplicate case ID");

    let known_tests: BTreeSet<_> = EXECUTABLE_TESTS.iter().copied().collect();
    let indexed_tests: BTreeSet<_> = corpus.cases.iter().map(|case| case.test.as_str()).collect();
    assert_eq!(indexed_tests, known_tests);

    let known_modes = BTreeSet::from([
        "config-dash",
        "file-diagnose-non-interactive",
        "file-headless",
        "file-headless-check-only",
        "file-non-interactive",
        "file-pty",
        "file-pty-interrupt",
        "file-pty-foreground-loss",
        "file-pty-job-control",
        "file-pty-no-fail",
        "stdin-config",
        "stdin-config-non-interactive",
    ]);
    for case in &corpus.cases {
        assert!(
            known_modes.contains(case.mode.as_str()),
            "{} has bad mode",
            case.id
        );
        assert!([0, 2, 3, 130].contains(&case.expected_exit));
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

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if path.starts_with("scripts/") {
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
fn config_args(path: &Path, extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        "--config".to_string(),
        path.to_string_lossy().into_owned(),
        "check".to_string(),
    ];
    args.extend(extra.iter().map(|value| (*value).to_string()));
    args
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn shell_command(args: &[String], stdin_path: Option<&Path>) -> Command {
    let mut command = Command::new("/bin/sh");
    let script = if stdin_path.is_some() {
        "exec \"$CHECKSY_TEST_BIN\" \"$@\" < \"$CHECKSY_STDIN_PATH\""
    } else {
        "exec \"$CHECKSY_TEST_BIN\" \"$@\""
    };
    command
        .arg("-c")
        .arg(script)
        .arg("checksy")
        .args(args)
        .env("CHECKSY_TEST_BIN", env!("CARGO_BIN_EXE_checksy"));
    if let Some(path) = stdin_path {
        command.env("CHECKSY_STDIN_PATH", path);
    }
    command
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn conventional_exit(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().expect("Unix status has code or signal"))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn assert_output_exit(case: &InteractiveFixCase, output: &Output) {
    assert_eq!(
        conventional_exit(output.status),
        case.expected_exit,
        "{}: stdout={:?} stderr={:?}",
        case.id,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn kill_group_if_present(pid: rustix::process::Pid) {
    match rustix::process::kill_process_group(pid, rustix::process::Signal::Kill) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => {}
        Err(error) => panic!("failed to kill process group {pid:?}: {error}"),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn set_cloexec<Fd: rustix::fd::AsFd>(fd: Fd) {
    let flags = rustix::io::fcntl_getfd(&fd).unwrap();
    rustix::io::fcntl_setfd(&fd, flags | rustix::io::FdFlags::CLOEXEC).unwrap();
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn dup_cloexec<Fd: rustix::fd::AsFd>(fd: Fd) -> std::os::fd::OwnedFd {
    rustix::io::fcntl_dupfd_cloexec(fd, 0).unwrap()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run_headless(args: &[String], stdin: Option<&[u8]>, env: &[(&str, &Path)]) -> Output {
    let _provisioning_guard = args
        .iter()
        .any(|argument| argument == "--fix")
        .then(support::provisioning_test_guard);
    let mut command = shell_command(args, None);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    for (name, value) in env {
        command.env(name, value);
    }
    // SAFETY: setsid is async-signal-safe and detaches the child from any
    // terminal owned by the test runner.
    unsafe {
        command.pre_exec(|| rustix::process::setsid().map(|_| ()).map_err(Into::into));
    }
    let mut child = command.spawn().unwrap();
    let pid = rustix::process::Pid::from_child(&child);
    if let Some(input) = stdin {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    let (send, receive) = mpsc::channel();
    thread::spawn(move || {
        let _ = send.send(child.wait_with_output());
    });
    match receive.recv_timeout(Duration::from_secs(12)) {
        Ok(output) => output.unwrap(),
        Err(error) => {
            kill_group_if_present(pid);
            match receive.recv_timeout(Duration::from_secs(3)) {
                Ok(output) => output.unwrap(),
                Err(_) => panic!("headless Checksy did not exit after watchdog: {error}"),
            }
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[derive(Debug, Eq, PartialEq)]
struct TerminalSnapshot {
    input: u64,
    output: u64,
    control: u64,
    local: u64,
    special: Vec<u8>,
    input_speed: u64,
    output_speed: u64,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn terminal_snapshot<Fd: rustix::fd::AsFd>(fd: Fd) -> TerminalSnapshot {
    let value = rustix::termios::tcgetattr(fd).unwrap();
    TerminalSnapshot {
        input: value.c_iflag as u64,
        output: value.c_oflag as u64,
        control: value.c_cflag as u64,
        local: value.c_lflag as u64,
        special: value.c_cc.to_vec(),
        input_speed: rustix::termios::cfgetispeed(&value) as u64,
        output_speed: rustix::termios::cfgetospeed(&value) as u64,
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
enum PtyRead {
    Bytes(Vec<u8>),
    Done,
    Error(std::io::Error),
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct PtyProcess {
    _provisioning_guard: File,
    pid: rustix::process::Pid,
    writer: File,
    initial_terminal: TerminalSnapshot,
    status: mpsc::Receiver<std::io::Result<ExitStatus>>,
    output: mpsc::Receiver<PtyRead>,
    collected_output: Vec<u8>,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct PtyResult {
    status: ExitStatus,
    output: Vec<u8>,
    initial_terminal: TerminalSnapshot,
    final_terminal: TerminalSnapshot,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl PtyProcess {
    fn spawn(mut command: Command) -> Self {
        let provisioning_guard = support::provisioning_test_guard();
        let master =
            rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
                .unwrap();
        set_cloexec(&master);
        rustix::pty::grantpt(&master).unwrap();
        rustix::pty::unlockpt(&master).unwrap();
        let slave_name = rustix::pty::ptsname(&master, Vec::new()).unwrap();
        let slave_path = Path::new(std::ffi::OsStr::from_bytes(slave_name.to_bytes()));
        let slave = rustix::fs::openat(
            rustix::fs::cwd(),
            slave_path,
            rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NOCTTY | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .unwrap();
        let child_stdin = dup_cloexec(&slave);
        let child_stdout = dup_cloexec(&slave);
        let child_stderr = dup_cloexec(&slave);
        let initial_terminal = terminal_snapshot(&master);
        command
            .stdin(Stdio::from(child_stdin))
            .stdout(Stdio::from(child_stdout))
            .stderr(Stdio::from(child_stderr));
        // SAFETY: the closure performs only async-signal-safe session and
        // descriptor ioctls using an already-open descriptor.
        unsafe {
            command.pre_exec(move || {
                rustix::process::setsid().map_err(std::io::Error::from)?;
                rustix::process::ioctl_tiocsctty(&slave).map_err(std::io::Error::from)
            });
        }
        let mut child = command.spawn().unwrap();
        drop(command);
        let pid = rustix::process::Pid::from_child(&child);

        let reader_fd = dup_cloexec(&master);
        let (output_send, output_receive) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = File::from(reader_fd);
            let mut buffer = [0_u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(length) => {
                        if output_send
                            .send(PtyRead::Bytes(buffer[..length].to_vec()))
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
                    Err(error) => {
                        let _ = output_send.send(PtyRead::Error(error));
                        return;
                    }
                }
            }
            let _ = output_send.send(PtyRead::Done);
        });

        let (status_send, status_receive) = mpsc::channel();
        thread::spawn(move || {
            let _ = status_send.send(child.wait());
        });

        Self {
            _provisioning_guard: provisioning_guard,
            pid,
            writer: File::from(master),
            initial_terminal,
            status: status_receive,
            output: output_receive,
            collected_output: Vec::new(),
        }
    }

    fn send_input(&mut self, input: &[u8]) {
        self.writer.write_all(input).unwrap();
        self.writer.flush().unwrap();
    }

    fn wait_for_output(&mut self, expected: &[u8], timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while !self
            .collected_output
            .windows(expected.len())
            .any(|window| window == expected)
        {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "PTY output omitted readiness sentinel"
            );
            match self.output.recv_timeout(remaining) {
                Ok(PtyRead::Bytes(bytes)) => self.collected_output.extend_from_slice(&bytes),
                Ok(PtyRead::Done) => panic!("PTY closed before readiness sentinel"),
                Ok(PtyRead::Error(error)) => panic!("PTY reader failed: {error}"),
                Err(error) => panic!("PTY readiness timed out: {error}"),
            }
        }
    }

    fn set_window_size(&self, rows: u16, columns: u16) {
        rustix::termios::tcsetwinsize(
            &self.writer,
            rustix::termios::Winsize {
                ws_row: rows,
                ws_col: columns,
                ws_xpixel: 0,
                ws_ypixel: 0,
            },
        )
        .unwrap();
    }

    fn finish(mut self, timeout: Duration) -> PtyResult {
        let status = match self.status.recv_timeout(timeout) {
            Ok(status) => status.unwrap(),
            Err(error) => {
                kill_group_if_present(self.pid);
                match self.status.recv_timeout(Duration::from_secs(3)) {
                    Ok(status) => status.unwrap(),
                    Err(_) => panic!("PTY Checksy did not exit after watchdog: {error}"),
                }
            }
        };
        let final_terminal = terminal_snapshot(&self.writer);
        drop(self.writer);

        let mut output = std::mem::take(&mut self.collected_output);
        loop {
            match self.output.recv_timeout(Duration::from_secs(3)) {
                Ok(PtyRead::Bytes(bytes)) => output.extend_from_slice(&bytes),
                Ok(PtyRead::Done) => break,
                Ok(PtyRead::Error(error)) => panic!("PTY reader failed: {error}"),
                Err(error) => panic!("PTY output did not close: {error}"),
            }
        }
        PtyResult {
            status,
            output,
            initial_terminal: self.initial_terminal,
            final_terminal,
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn spawn_config_pty(path: &Path, extra: &[&str], env: &[(&str, &Path)]) -> PtyProcess {
    let args = config_args(path, extra);
    let mut command = shell_command(&args, None);
    for (name, value) in env {
        command.env(name, value);
    }
    PtyProcess::spawn(command)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn spawn_stdin_config_pty(path: &Path, args: &[&str], env: &[(&str, &Path)]) -> PtyProcess {
    let args: Vec<_> = args.iter().map(|value| (*value).to_string()).collect();
    let mut command = shell_command(&args, Some(path));
    for (name, value) in env {
        command.env(name, value);
    }
    PtyProcess::spawn(command)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn spawn_foreground_loss_pty(
    path: &Path,
    command_socket: &Path,
    ready_socket: &Path,
    unexpected_later: &Path,
) -> PtyProcess {
    let args = config_args(path, &["--fix"]);
    let mut command = Command::new("bash");
    command
        .arg("-c")
        .arg(concat!(
            "set -m\n",
            "\"$CHECKSY_FOREGROUND_HELPER\" --ignored --exact ",
            "interactive_foreground_thief_helper --nocapture ",
            "</dev/null >/dev/null 2>&1 &\n",
            "exec \"$CHECKSY_TEST_BIN\" \"$@\"\n"
        ))
        .arg("checksy")
        .args(args)
        .env("CHECKSY_TEST_BIN", env!("CARGO_BIN_EXE_checksy"))
        .env(
            "CHECKSY_FOREGROUND_HELPER",
            std::env::current_exe().unwrap(),
        )
        .env("CHECKSY_FOREGROUND_COMMAND_SOCKET", command_socket)
        .env("CHECKSY_FOREGROUND_READY_SOCKET", ready_socket)
        .env("CHECKSY_UNEXPECTED_LATER_COMMAND", unexpected_later);
    PtyProcess::spawn(command)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn assert_pty_exit(case: &InteractiveFixCase, result: &PtyResult) {
    assert_eq!(
        conventional_exit(result.status),
        case.expected_exit,
        "{}: PTY output={:?}",
        case.id,
        String::from_utf8_lossy(&result.output)
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn assert_terminal_restored(result: &PtyResult) {
    assert_eq!(result.final_terminal, result.initial_terminal);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn interactive_fix_reads_stdin_and_dev_tty_then_rechecks() {
    let corpus = corpus();
    let case = case(&corpus, "interactive-success");
    let copy = writable_fixture_copy();
    let mut process = spawn_config_pty(&copy.path().join(&case.fixture), &["--fix"], &[]);
    process.send_input(b"stdin-answer\ntty-answer\n");
    let result = process.finish(Duration::from_secs(12));
    assert_pty_exit(case, &result);
    assert_terminal_restored(&result);
    let output = String::from_utf8_lossy(&result.output);
    for expected in [
        "interactive stdin prompt",
        "interactive tty prompt",
        "interactive repair complete",
        "All rules validated",
    ] {
        assert!(
            output.contains(expected),
            "output omitted {expected:?}: {output:?}"
        );
    }
    assert!(copy.path().join(".interactive-fixed").is_file());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn interactive_fix_inherits_only_standard_pty_descriptors() {
    let corpus = corpus();
    let case = case(&corpus, "no-extra-pty-descriptors");
    let copy = writable_fixture_copy();
    let process = spawn_config_pty(&copy.path().join(&case.fixture), &["--fix"], &[]);
    let result = process.finish(Duration::from_secs(10));
    assert_pty_exit(case, &result);
    assert_terminal_restored(&result);
    assert!(copy.path().join(".interactive-fixed").is_file());
    assert!(!String::from_utf8_lossy(&result.output)
        .contains("unexpected inherited terminal descriptor"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn passing_interactive_fix_never_probes_for_terminal() {
    let corpus = corpus();
    let file_case = case(&corpus, "passing-file-headless");
    let file_copy = writable_fixture_copy();
    let file_marker = file_copy.path().join("unexpected-interactive");
    let output = run_headless(
        &config_args(&file_copy.path().join(&file_case.fixture), &["--fix"]),
        None,
        &[("CHECKSY_UNEXPECTED_INTERACTIVE", &file_marker)],
    );
    assert_output_exit(file_case, &output);
    assert!(!file_marker.exists());
    assert!(output.stderr.is_empty());

    let stdin_case = case(&corpus, "passing-stdin-headless");
    let stdin_copy = writable_fixture_copy();
    let stdin_marker = stdin_copy.path().join("unexpected-interactive");
    let document = fs::read(stdin_copy.path().join(&stdin_case.fixture)).unwrap();
    let output = run_headless(
        &["--stdin-config".into(), "check".into(), "--fix".into()],
        Some(&document),
        &[("CHECKSY_UNEXPECTED_INTERACTIVE", &stdin_marker)],
    );
    assert_output_exit(stdin_case, &output);
    assert!(!stdin_marker.exists());
    assert!(output.stderr.is_empty());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn unavailable_interactive_fix_is_a_compliance_result() {
    let corpus = corpus();
    for (id, extra, expected_text) in [
        (
            "needed-file-headless",
            vec!["--fix"],
            "no usable controlling terminal",
        ),
        (
            "needed-explicit-non-interactive",
            vec!["--fix", "--non-interactive"],
            "--non-interactive prohibits terminal use",
        ),
    ] {
        let case = case(&corpus, id);
        let copy = writable_fixture_copy();
        let unexpected = copy.path().join("unexpected-interactive");
        let later = copy.path().join("later-rule");
        let output = run_headless(
            &config_args(&copy.path().join(&case.fixture), &extra),
            None,
            &[
                ("CHECKSY_UNEXPECTED_INTERACTIVE", &unexpected),
                ("CHECKSY_LATER_RULE_MARKER", &later),
            ],
        );
        assert_output_exit(case, &output);
        assert!(!unexpected.exists());
        assert!(later.is_file());
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected_text));
    }

    let copy = writable_fixture_copy();
    let unexpected = copy.path().join("unexpected-interactive");
    let later = copy.path().join("later-rule");
    let output = run_headless(
        &config_args(
            &copy.path().join("unavailable-interactive.yaml"),
            &["--fix", "--non-interactive", "--no-fail"],
        ),
        None,
        &[
            ("CHECKSY_UNEXPECTED_INTERACTIVE", &unexpected),
            ("CHECKSY_LATER_RULE_MARKER", &later),
        ],
    );
    assert_eq!(conventional_exit(output.status), 0);
    assert!(!unexpected.exists());
    assert!(later.is_file());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn check_only_ignores_interactive_fix() {
    let corpus = corpus();
    let case = case(&corpus, "needed-check-only");
    let copy = writable_fixture_copy();
    let unexpected = copy.path().join("unexpected-interactive");
    let later = copy.path().join("later-rule");
    let output = run_headless(
        &config_args(&copy.path().join(&case.fixture), &[]),
        None,
        &[
            ("CHECKSY_UNEXPECTED_INTERACTIVE", &unexpected),
            ("CHECKSY_LATER_RULE_MARKER", &later),
        ],
    );
    assert_output_exit(case, &output);
    assert!(!unexpected.exists());
    assert!(later.is_file());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("interactive repair required"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn deprecated_diagnose_accepts_non_interactive() {
    let corpus = corpus();
    let case = case(&corpus, "diagnose-explicit-non-interactive");
    let copy = writable_fixture_copy();
    let unexpected = copy.path().join("unexpected-interactive");
    let later = copy.path().join("later-rule");
    let args = vec![
        "--config".to_string(),
        copy.path()
            .join(&case.fixture)
            .to_string_lossy()
            .into_owned(),
        "diagnose".to_string(),
        "--fix".to_string(),
        "--non-interactive".to_string(),
    ];
    let output = run_headless(
        &args,
        None,
        &[
            ("CHECKSY_UNEXPECTED_INTERACTIVE", &unexpected),
            ("CHECKSY_LATER_RULE_MARKER", &later),
        ],
    );
    assert_output_exit(case, &output);
    assert!(!unexpected.exists());
    assert!(later.is_file());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("diagnose\" is deprecated"));
    assert!(stderr.contains("--non-interactive prohibits terminal use"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn stdin_configuration_never_opens_a_terminal() {
    let corpus = corpus();
    for (id, args) in [
        (
            "needed-stdin-config",
            vec!["--stdin-config", "check", "--fix"],
        ),
        (
            "needed-config-dash",
            vec!["--config", "-", "check", "--fix"],
        ),
    ] {
        let case = case(&corpus, id);
        let copy = writable_fixture_copy();
        let unexpected = copy.path().join("unexpected-interactive");
        let later = copy.path().join("later-rule");
        let process = spawn_stdin_config_pty(
            &copy.path().join(&case.fixture),
            &args,
            &[
                ("CHECKSY_UNEXPECTED_INTERACTIVE", &unexpected),
                ("CHECKSY_LATER_RULE_MARKER", &later),
            ],
        );
        let result = process.finish(Duration::from_secs(10));
        assert_pty_exit(case, &result);
        assert_terminal_restored(&result);
        assert!(!unexpected.exists());
        assert!(later.is_file());
        let output = String::from_utf8_lossy(&result.output);
        assert!(output.contains("stdin configuration is always non-interactive"));
        assert!(!output.contains("--non-interactive prohibits terminal use"));
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn stdin_diagnostic_takes_precedence() {
    let corpus = corpus();
    let case = case(&corpus, "needed-stdin-precedence");
    let copy = writable_fixture_copy();
    let unexpected = copy.path().join("unexpected-interactive");
    let later = copy.path().join("later-rule");
    let process = spawn_stdin_config_pty(
        &copy.path().join(&case.fixture),
        &["--stdin-config", "check", "--fix", "--non-interactive"],
        &[
            ("CHECKSY_UNEXPECTED_INTERACTIVE", &unexpected),
            ("CHECKSY_LATER_RULE_MARKER", &later),
        ],
    );
    let result = process.finish(Duration::from_secs(10));
    assert_pty_exit(case, &result);
    assert_terminal_restored(&result);
    assert!(!unexpected.exists());
    assert!(later.is_file());
    let output = String::from_utf8_lossy(&result.output);
    assert!(output.contains("stdin configuration is always non-interactive"));
    assert!(!output.contains("--non-interactive prohibits terminal use"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn ordinary_fix_runs_under_non_interactive() {
    let corpus = corpus();
    let case = case(&corpus, "ordinary-fix-non-interactive");
    let copy = writable_fixture_copy();
    let output = run_headless(
        &config_args(
            &copy.path().join(&case.fixture),
            &["--fix", "--non-interactive"],
        ),
        None,
        &[],
    );
    assert_output_exit(case, &output);
    assert!(copy.path().join(".ordinary-fixed").is_file());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn failed_interactive_fix_continues_and_honors_no_fail() {
    let corpus = corpus();
    for (id, extra) in [
        ("failed-repair-continues", vec!["--fix"]),
        ("failed-repair-no-fail", vec!["--fix", "--no-fail"]),
        ("failed-repair-below-threshold", vec!["--fix"]),
    ] {
        let case = case(&corpus, id);
        let copy = writable_fixture_copy();
        let later = copy.path().join("later-rule");
        let process = spawn_config_pty(
            &copy.path().join(&case.fixture),
            &extra,
            &[("CHECKSY_LATER_RULE_MARKER", &later)],
        );
        let result = process.finish(Duration::from_secs(10));
        assert_pty_exit(case, &result);
        assert_terminal_restored(&result);
        assert!(later.is_file());
        assert!(copy.path().join(".failed-interactive-started").is_file());
        assert!(!copy.path().join(".unexpected-recheck").exists());
        let output = String::from_utf8_lossy(&result.output);
        assert_eq!(
            output
                .matches("failed interactive repair stdout sentinel")
                .count(),
            1
        );
        assert_eq!(
            output
                .matches("failed interactive repair stderr sentinel")
                .count(),
            1
        );
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
fn helper_command(role: &str, directory: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--ignored",
            "--exact",
            "interactive_process_tree_helper",
            "--nocapture",
        ])
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
#[test]
#[ignore = "isolated workload invoked by interactive-fix contract tests"]
fn interactive_process_tree_helper() {
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
                if watchdog_wait.recv_timeout(Duration::from_secs(12)).is_err() {
                    kill_group_if_present(leader);
                }
            });
            let mut child = helper_command("child", &directory).spawn().unwrap();
            wait_for_helper_ready(&mut child);

            let ready_path =
                PathBuf::from(std::env::var_os("CHECKSY_READY_SOCKET").expect("ready socket"));
            let ready = UnixDatagram::unbound().unwrap();
            set_cloexec(&ready);
            ready
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
#[test]
#[ignore = "isolated same-session helper invoked by the foreground-loss contract test"]
fn interactive_foreground_thief_helper() {
    let command_path =
        PathBuf::from(std::env::var_os("CHECKSY_FOREGROUND_COMMAND_SOCKET").unwrap());
    let ready_path = PathBuf::from(std::env::var_os("CHECKSY_FOREGROUND_READY_SOCKET").unwrap());
    let command = UnixDatagram::bind(&command_path).unwrap();
    set_cloexec(&command);
    command
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    let ready = UnixDatagram::unbound().unwrap();
    set_cloexec(&ready);
    let process_group = rustix::process::getpgrp();
    ready
        .send_to(
            format!("ready:{}", process_group.as_raw_nonzero()).as_bytes(),
            &ready_path,
        )
        .unwrap();

    let mut trigger = [0_u8; 16];
    command.recv(&mut trigger).unwrap();
    // SAFETY: this helper is a dedicated subprocess. Ignoring SIGTTOU permits
    // its same-session background process group to claim the outer PTY.
    let previous = unsafe { libc::signal(libc::SIGTTOU, libc::SIG_IGN) };
    assert_ne!(previous, libc::SIG_ERR);
    let terminal = rustix::fs::openat(
        rustix::fs::cwd(),
        "/dev/tty",
        rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NOCTTY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .unwrap();
    #[cfg(target_os = "linux")]
    let raw_process_group =
        i32::try_from(rustix::process::Pid::as_raw(Some(process_group))).unwrap();
    #[cfg(target_os = "macos")]
    let raw_process_group = rustix::process::Pid::as_raw(Some(process_group));
    // SAFETY: the descriptor is the helper's controlling terminal and the
    // process-group ID belongs to the same session.
    if unsafe { libc::tcsetpgrp(terminal.as_raw_fd(), raw_process_group) } != 0 {
        let error = std::io::Error::last_os_error();
        ready
            .send_to(format!("error:{error}").as_bytes(), &ready_path)
            .unwrap();
        panic!("foreground helper could not claim the terminal: {error}");
    }
    drop(terminal);
    ready.send_to(b"stolen", &ready_path).unwrap();

    loop {
        thread::park();
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn wait_for_tree_ready(socket: &UnixDatagram) -> rustix::process::Pid {
    socket
        .set_read_timeout(Some(Duration::from_secs(7)))
        .unwrap();
    let mut message = [0_u8; 64];
    let length = socket
        .recv(&mut message)
        .expect("interactive tree readiness");
    let raw = std::str::from_utf8(&message[..length])
        .unwrap()
        .strip_prefix("ready:")
        .unwrap()
        .parse::<i32>()
        .unwrap();
    // SAFETY: this is a positive PID supplied directly by the helper process.
    #[cfg(target_os = "linux")]
    unsafe {
        rustix::process::Pid::from_raw(raw.try_into().unwrap()).unwrap()
    }
    #[cfg(target_os = "macos")]
    unsafe {
        rustix::process::Pid::from_raw(raw).unwrap()
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct HelperCleanup(Option<rustix::process::Pid>);

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Drop for HelperCleanup {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            kill_group_if_present(pid);
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn interactive_fix_timeout_is_operational_and_fail_fast() {
    let corpus = corpus();
    let case = case(&corpus, "interactive-timeout-fail-fast");
    let copy = writable_fixture_copy();
    let socket_path = copy.path().join("ready.sock");
    let socket = UnixDatagram::bind(&socket_path).unwrap();
    set_cloexec(&socket);
    let started = copy.path().join("interactive-started");
    let recheck = copy.path().join("unexpected-recheck");
    let later = copy.path().join("unexpected-later");
    let helper = std::env::current_exe().unwrap();
    let env = [
        ("CHECKSY_PROCESS_HELPER", helper.as_path()),
        ("CHECKSY_HELPER_DIR", copy.path()),
        ("CHECKSY_READY_SOCKET", socket_path.as_path()),
        ("CHECKSY_INTERACTIVE_FIX_STARTED", started.as_path()),
        ("CHECKSY_UNEXPECTED_RECHECK", recheck.as_path()),
        ("CHECKSY_UNEXPECTED_LATER_COMMAND", later.as_path()),
    ];
    let process = spawn_config_pty(&copy.path().join(&case.fixture), &["--fix"], &env);
    let leader = wait_for_tree_ready(&socket);
    let _cleanup = HelperCleanup(Some(leader));
    let result = process.finish(Duration::from_secs(15));
    assert_pty_exit(case, &result);
    assert_terminal_restored(&result);
    assert!(started.is_file());
    assert!(!recheck.exists());
    assert!(!later.exists());
    assert_lock_reacquirable(&copy.path().join("child.lock"));
    assert_lock_reacquirable(&copy.path().join("grandchild.lock"));
    let output = String::from_utf8_lossy(&result.output);
    assert!(output.contains("interactive repair stdout before supervision event"));
    assert!(output.contains("interactive repair stderr before supervision event"));
    assert!(output.to_ascii_lowercase().contains("timed out"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn ctrl_c_cleans_interactive_fix_tree_and_resignals_checksy() {
    let corpus = corpus();
    let case = case(&corpus, "interactive-parent-interrupt");
    let copy = writable_fixture_copy();
    let socket_path = copy.path().join("ready.sock");
    let socket = UnixDatagram::bind(&socket_path).unwrap();
    set_cloexec(&socket);
    let started = copy.path().join("interactive-started");
    let unused_recheck = copy.path().join("unused-recheck");
    let unused_later = copy.path().join("unused-later");
    let helper = std::env::current_exe().unwrap();
    let env = [
        ("CHECKSY_PROCESS_HELPER", helper.as_path()),
        ("CHECKSY_HELPER_DIR", copy.path()),
        ("CHECKSY_READY_SOCKET", socket_path.as_path()),
        ("CHECKSY_INTERACTIVE_FIX_STARTED", started.as_path()),
        ("CHECKSY_UNEXPECTED_RECHECK", unused_recheck.as_path()),
        ("CHECKSY_UNEXPECTED_LATER_COMMAND", unused_later.as_path()),
    ];
    let mut process = spawn_config_pty(&copy.path().join(&case.fixture), &["--fix"], &env);
    let leader = wait_for_tree_ready(&socket);
    let _cleanup = HelperCleanup(Some(leader));
    let interrupted_at = Instant::now();
    process.send_input(&[3]);
    let result = process.finish(Duration::from_secs(15));
    assert_pty_exit(case, &result);
    assert_terminal_restored(&result);
    assert_eq!(result.status.signal(), Some(libc::SIGINT));
    assert!(
        interrupted_at.elapsed() < Duration::from_secs(8),
        "the runner did not clean up before the 12-second helper watchdog"
    );
    assert_lock_reacquirable(&copy.path().join("child.lock"));
    assert_lock_reacquirable(&copy.path().join("grandchild.lock"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn outer_job_control_cleans_the_tree_without_suspending_checksy() {
    let corpus = corpus();
    let case = case(&corpus, "outer-job-control");
    let copy = writable_fixture_copy();
    let socket_path = copy.path().join("ready.sock");
    let socket = UnixDatagram::bind(&socket_path).unwrap();
    set_cloexec(&socket);
    let started = copy.path().join("interactive-started");
    let helper = std::env::current_exe().unwrap();
    let unused_recheck = copy.path().join("unused-recheck");
    let unused_later = copy.path().join("unused-later");
    let env = [
        ("CHECKSY_PROCESS_HELPER", helper.as_path()),
        ("CHECKSY_HELPER_DIR", copy.path()),
        ("CHECKSY_READY_SOCKET", socket_path.as_path()),
        ("CHECKSY_INTERACTIVE_FIX_STARTED", started.as_path()),
        ("CHECKSY_UNEXPECTED_RECHECK", unused_recheck.as_path()),
        ("CHECKSY_UNEXPECTED_LATER_COMMAND", unused_later.as_path()),
    ];
    let process = spawn_config_pty(&copy.path().join(&case.fixture), &["--fix"], &env);
    let leader = wait_for_tree_ready(&socket);
    let _cleanup = HelperCleanup(Some(leader));
    let interrupted_at = Instant::now();
    rustix::process::kill_process(process.pid, rustix::process::Signal::Tstp).unwrap();
    let result = process.finish(Duration::from_secs(15));
    assert_pty_exit(case, &result);
    assert_terminal_restored(&result);
    assert!(started.is_file());
    assert!(
        interrupted_at.elapsed() < Duration::from_secs(12),
        "Checksy suspended until the helper watchdog"
    );
    assert_lock_reacquirable(&copy.path().join("child.lock"));
    assert_lock_reacquirable(&copy.path().join("grandchild.lock"));
    assert!(String::from_utf8_lossy(&result.output).contains("job-control signal"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn foreground_loss_stops_outer_relay_and_cleans_the_repair() {
    let corpus = corpus();
    let case = case(&corpus, "foreground-loss");
    let copy = writable_fixture_copy();
    let command_path = copy.path().join("foreground-command.sock");
    let ready_path = copy.path().join("foreground-ready.sock");
    let ready = UnixDatagram::bind(&ready_path).unwrap();
    set_cloexec(&ready);
    let unexpected_later = copy.path().join("unexpected-later");
    let mut process = spawn_foreground_loss_pty(
        &copy.path().join(&case.fixture),
        &command_path,
        &ready_path,
        &unexpected_later,
    );
    let helper = wait_for_tree_ready(&ready);
    let _cleanup = HelperCleanup(Some(helper));
    process.wait_for_output(b"foreground loss repair ready", Duration::from_secs(5));

    let trigger = UnixDatagram::unbound().unwrap();
    set_cloexec(&trigger);
    trigger.send_to(b"steal", &command_path).unwrap();
    let mut acknowledgement = [0_u8; 16];
    let length = ready.recv(&mut acknowledgement).unwrap();
    assert_eq!(&acknowledgement[..length], b"stolen");

    let result = process.finish(Duration::from_secs(10));
    assert_pty_exit(case, &result);
    assert_terminal_restored(&result);
    assert!(!unexpected_later.exists());
    let output = String::from_utf8_lossy(&result.output);
    assert!(
        output.contains("lost ownership of the controlling terminal"),
        "foreground-loss output: {output:?}"
    );
    assert!(!output.contains("forbidden relay after foreground loss"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn interactive_job_control_suspension_is_operational() {
    let corpus = corpus();
    let case = case(&corpus, "interactive-suspension");
    let copy = writable_fixture_copy();
    let unexpected = copy.path().join("unexpected-later");
    let process = spawn_config_pty(
        &copy.path().join(&case.fixture),
        &["--fix"],
        &[("CHECKSY_UNEXPECTED_LATER_COMMAND", &unexpected)],
    );
    let started_at = Instant::now();
    let result = process.finish(Duration::from_secs(12));
    assert_pty_exit(case, &result);
    assert_terminal_restored(&result);
    assert!(!unexpected.exists());
    let output = String::from_utf8_lossy(&result.output);
    assert_eq!(output.matches("interactive suspension sentinel").count(), 1);
    assert!(
        started_at.elapsed() < Duration::from_secs(10),
        "job-control suspension reached the outer watchdog"
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn interactive_fix_streams_output_and_restores_terminal() {
    let corpus = corpus();
    let case = case(&corpus, "terminal-restoration");
    let copy = writable_fixture_copy();
    let process = spawn_config_pty(&copy.path().join(&case.fixture), &["--fix"], &[]);
    let result = process.finish(Duration::from_secs(10));
    assert_pty_exit(case, &result);
    assert_terminal_restored(&result);
    assert_eq!(
        String::from_utf8_lossy(&result.output)
            .matches("interactive live-output sentinel")
            .count(),
        1
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn interactive_fix_receives_outer_terminal_resize() {
    let corpus = corpus();
    let case = case(&corpus, "terminal-resize");
    let copy = writable_fixture_copy();
    let mut process = spawn_config_pty(&copy.path().join(&case.fixture), &["--fix"], &[]);
    process.wait_for_output(b"window resize ready", Duration::from_secs(5));
    process.set_window_size(37, 91);
    process.send_input(b"continue\n");
    let result = process.finish(Duration::from_secs(10));
    assert_pty_exit(case, &result);
    assert_terminal_restored(&result);
    assert!(copy.path().join(".interactive-fixed").is_file());
    assert!(!String::from_utf8_lossy(&result.output).contains("unexpected inner PTY size"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn precondition_interactive_fix_uses_the_same_workflow() {
    let corpus = corpus();
    let case = case(&corpus, "precondition-interactive");
    let copy = writable_fixture_copy();
    let later = copy.path().join("later-rule");
    let mut process = spawn_config_pty(
        &copy.path().join(&case.fixture),
        &["--fix"],
        &[("CHECKSY_LATER_RULE_MARKER", &later)],
    );
    process.send_input(b"stdin-answer\ntty-answer\n");
    let result = process.finish(Duration::from_secs(12));
    assert_pty_exit(case, &result);
    assert_terminal_restored(&result);
    assert!(copy.path().join(".interactive-fixed").is_file());
    assert!(later.is_file());
}
