use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

mod support;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct P0AcceptanceCorpus {
    schema_version: u64,
    cases: Vec<P0AcceptanceCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct P0AcceptanceCase {
    id: String,
    fixture: String,
    assets: Vec<String>,
    modes: Vec<String>,
    test: String,
}

const EXECUTABLE_TESTS: &[&str] = &[
    "core_workflow_proves_headless_file_and_stdin_provisioning",
    "interactive_lifecycle_distinguishes_pty_headless_and_stdin",
    "file_and_stdin_provisioning_contend_on_the_same_semaphore",
    "predicate_timeout_cleans_managed_descendants",
    "invalid_configuration_precedes_commands_and_lock",
];

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join("p0-acceptance")
}

fn corpus() -> P0AcceptanceCorpus {
    let document = fs::read_to_string(fixture_root().join("cases.yaml")).unwrap();
    let corpus: P0AcceptanceCorpus = serde_yaml::from_str(&document).unwrap();
    assert_eq!(corpus.schema_version, 1);
    corpus
}

fn case<'a>(corpus: &'a P0AcceptanceCorpus, id: &str) -> &'a P0AcceptanceCase {
    corpus
        .cases
        .iter()
        .find(|case| case.id == id)
        .unwrap_or_else(|| panic!("P0 acceptance corpus omitted {id:?}"))
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
fn corpus_is_closed_unique_network_free_and_mapped_to_tests() {
    let corpus = corpus();
    assert_eq!(corpus.cases.len(), 5, "the P0 gate has exactly five cases");

    let ids: BTreeSet<_> = corpus.cases.iter().map(|case| case.id.as_str()).collect();
    assert_eq!(ids.len(), corpus.cases.len(), "duplicate case ID");
    assert_eq!(
        ids,
        BTreeSet::from([
            "core-flow",
            "file-stdin-lock-contention",
            "interactive-lifecycle",
            "invalid-preflight",
            "predicate-tree-timeout",
        ])
    );

    let tests: BTreeSet<_> = corpus.cases.iter().map(|case| case.test.as_str()).collect();
    assert_eq!(tests, EXECUTABLE_TESTS.iter().copied().collect());

    let fixture_paths: BTreeSet<_> = corpus
        .cases
        .iter()
        .map(|case| case.fixture.as_str())
        .collect();
    assert_eq!(
        fixture_paths.len(),
        corpus.cases.len(),
        "each case needs its own configuration"
    );

    let known_modes = BTreeSet::from([
        "file-headless",
        "file-holder-stdin-contender",
        "file-invalid",
        "file-invalid-lock-held",
        "file-no-fail-timeout",
        "file-non-interactive",
        "file-pty",
        "stdin-config",
        "stdin-holder-file-contender",
        "stdin-invalid",
        "stdin-with-pty",
    ]);
    for case in &corpus.cases {
        assert!(!case.modes.is_empty(), "{} has no execution mode", case.id);
        assert!(
            case.assets.is_empty(),
            "{} unexpectedly has assets",
            case.id
        );
        let unique_modes: BTreeSet<_> = case.modes.iter().map(String::as_str).collect();
        assert_eq!(
            unique_modes.len(),
            case.modes.len(),
            "{} repeats an execution mode",
            case.id
        );
        for mode in &case.modes {
            assert!(
                known_modes.contains(mode.as_str()),
                "{} has unknown mode {mode:?}",
                case.id
            );
        }
        let path = Path::new(&case.fixture);
        assert!(path.is_relative(), "{} uses an absolute fixture", case.id);
        assert!(
            !path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir)),
            "{} escapes the corpus",
            case.id
        );

        let (fixture, modes, test) = match case.id.as_str() {
            "core-flow" => (
                "core-flow.yaml",
                &["file-non-interactive", "stdin-config"][..],
                "core_workflow_proves_headless_file_and_stdin_provisioning",
            ),
            "interactive-lifecycle" => (
                "interactive-flow.yaml",
                &["file-pty", "file-headless", "stdin-with-pty"][..],
                "interactive_lifecycle_distinguishes_pty_headless_and_stdin",
            ),
            "file-stdin-lock-contention" => (
                "blocking-flow.yaml",
                &["file-holder-stdin-contender", "stdin-holder-file-contender"][..],
                "file_and_stdin_provisioning_contend_on_the_same_semaphore",
            ),
            "predicate-tree-timeout" => (
                "predicate-tree-timeout.yaml",
                &["file-no-fail-timeout"][..],
                "predicate_timeout_cleans_managed_descendants",
            ),
            "invalid-preflight" => (
                "invalid-preflight.yaml",
                &["file-invalid", "stdin-invalid", "file-invalid-lock-held"][..],
                "invalid_configuration_precedes_commands_and_lock",
            ),
            _ => unreachable!("the exact ID assertion rejects unknown cases"),
        };
        assert_eq!(case.fixture, fixture, "{} has the wrong fixture", case.id);
        assert_eq!(case.modes, modes, "{} has the wrong modes", case.id);
        assert_eq!(case.test, test, "{} has the wrong test", case.id);
    }

    let root = fixture_root();
    let mut actual_files = BTreeSet::new();
    collect_files(&root, &root, &mut actual_files);
    let indexed_files: BTreeSet<_> = corpus
        .cases
        .iter()
        .flat_map(|case| std::iter::once(&case.fixture).chain(case.assets.iter()))
        .cloned()
        .collect();
    let mut fixture_files = actual_files.clone();
    fixture_files.remove("README.md");
    fixture_files.remove("cases.yaml");
    assert_eq!(indexed_files, fixture_files, "fixture index is not closed");

    for path in actual_files {
        let bytes = fs::read(root.join(&path)).unwrap();
        assert!(!bytes.contains(&b'\r'), "{path} must use LF endings");
        assert_eq!(bytes.last(), Some(&b'\n'), "{path} needs a final newline");

        if indexed_files.contains(&path) {
            let text = String::from_utf8(bytes).unwrap().to_ascii_lowercase();
            for forbidden in ["http://", "https://", "ssh://", "git@", "remote:"] {
                assert!(
                    !text.contains(forbidden),
                    "{path} contains forbidden network/remote token {forbidden:?}"
                );
            }
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix {
    use super::*;
    use std::ffi::CString;
    use std::fs::{File, OpenOptions};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::net::UnixDatagram;
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, ExitStatus, Output, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

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

    fn file_command(root: &Path, fixture: &str, extra: &[&str]) -> Command {
        let mut command = checksy();
        command
            .current_dir(root)
            .arg("--config")
            .arg(root.join(fixture))
            .arg("check")
            .args(extra);
        command
    }

    fn stdin_command(root: &Path, extra: &[&str]) -> Command {
        let mut command = checksy();
        command
            .current_dir(root)
            .arg("--stdin-config")
            .arg("check")
            .args(extra);
        command
    }

    fn kill_group_if_present(pid: rustix::process::Pid) {
        match rustix::process::kill_process_group(pid, rustix::process::Signal::Kill) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => panic!("failed to kill process group {pid:?}: {error}"),
        }
    }

    fn cleanup_process_group(mut child: Child, pid: rustix::process::Pid) {
        match rustix::process::kill_process_group(pid, rustix::process::Signal::Kill) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(_) => {
                let _ = child.kill();
            }
        }
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(child.wait());
        });
        let _ = receiver.recv_timeout(Duration::from_secs(3));
    }

    struct ChildProcessGroup {
        child: Option<Child>,
        pid: rustix::process::Pid,
    }

    impl ChildProcessGroup {
        fn new(child: Child) -> Self {
            let pid = rustix::process::Pid::from_child(&child);
            Self {
                child: Some(child),
                pid,
            }
        }

        fn child_mut(&mut self) -> &mut Child {
            self.child.as_mut().unwrap()
        }

        fn disarm(mut self) -> Child {
            self.child.take().unwrap()
        }
    }

    impl Drop for ChildProcessGroup {
        fn drop(&mut self) {
            if let Some(child) = self.child.take() {
                cleanup_process_group(child, self.pid);
            }
        }
    }

    fn wait_output_bounded(child: Child, timeout: Duration) -> Output {
        let pid = rustix::process::Pid::from_child(&child);
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(child.wait_with_output());
        });
        match receiver.recv_timeout(timeout) {
            Ok(output) => output.unwrap(),
            Err(error) => {
                kill_group_if_present(pid);
                match receiver.recv_timeout(Duration::from_secs(3)) {
                    Ok(output) => output.unwrap(),
                    Err(_) => panic!("Checksy did not exit after watchdog: {error}"),
                }
            }
        }
    }

    fn run_headless(mut command: Command, input: Option<&[u8]>) -> Output {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        if input.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        // SAFETY: setsid is async-signal-safe and detaches Checksy from the
        // test runner's terminal while also creating a watchdog process group.
        unsafe {
            command.pre_exec(|| rustix::process::setsid().map(|_| ()).map_err(Into::into));
        }
        let mut child = ChildProcessGroup::new(command.spawn().unwrap());
        if let Some(input) = input {
            child
                .child_mut()
                .stdin
                .take()
                .unwrap()
                .write_all(input)
                .unwrap();
        }
        wait_output_bounded(child.disarm(), COMMAND_TIMEOUT)
    }

    fn exit_code(output: &Output) -> i32 {
        output.status.code().unwrap_or_else(|| {
            panic!(
                "Checksy exited by signal: stdout={:?} stderr={:?}",
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

    fn assert_exit(output: &Output, expected: i32) {
        assert_eq!(exit_code(output), expected, "{}", output_context(output));
    }

    fn assert_ordered(text: &str, expected: &[&str]) {
        let mut previous = 0;
        for value in expected {
            let position = text[previous..]
                .find(value)
                .unwrap_or_else(|| panic!("output omitted {value:?}: {text:?}"));
            previous += position + value.len();
        }
    }

    fn set_core_environment(command: &mut Command, root: &Path) {
        command
            .env("CHECKSY_P0_TRACE", root.join("trace"))
            .env("CHECKSY_P0_FIXED", root.join("fixed"))
            .env(
                "CHECKSY_P0_MISSING_COMMAND",
                root.join("guaranteed-missing-command"),
            );
    }

    #[test]
    fn core_workflow_proves_headless_file_and_stdin_provisioning() {
        let _serial = support::provisioning_test_guard();
        let corpus = corpus();
        let indexed = case(&corpus, "core-flow");

        for stdin in [false, true] {
            let copy = writable_fixture_copy();
            let document = fs::read(copy.path().join(&indexed.fixture)).unwrap();
            let mut command = if stdin {
                stdin_command(copy.path(), &["--fix"])
            } else {
                file_command(
                    copy.path(),
                    &indexed.fixture,
                    &["--fix", "--non-interactive"],
                )
            };
            set_core_environment(&mut command, copy.path());
            let output = run_headless(command, stdin.then_some(document.as_slice()));
            assert_exit(&output, 0);
            assert!(output.stderr.is_empty(), "{}", output_context(&output));

            assert_eq!(
                fs::read_to_string(copy.path().join("trace")).unwrap(),
                concat!(
                    "skip-predicate\n",
                    "exit-1-predicate\n",
                    "ordinary-check\n",
                    "ordinary-fix\n",
                    "ordinary-check\n",
                    "exit-23-predicate\n",
                    "exit-23-check\n",
                    "exit-127-predicate\n",
                    "exit-127-check\n",
                    "interactive-pass-check\n",
                ),
                "the predicate/check/fix order changed"
            );
            assert!(copy.path().join("fixed").is_file());
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert_ordered(
                &stdout,
                &[
                    "⏭️ skip all work (skipped)",
                    "⚠️  completed nonzero provisions",
                    "✅ completed nonzero provisions fix",
                    "✅ completed nonzero provisions",
                    "✅ exit twenty-three still checks",
                    "✅ shell exit one-twenty-seven still checks",
                    "✅ passing interactive stays headless",
                    "😎 All applicable rules validated; 1 skipped",
                ],
            );
            for forbidden in [
                "hidden skip predicate",
                "hidden exit one",
                "hidden exit twenty-three",
                "hidden exit one-twenty-seven",
                "unexpected-skip-check",
                "unexpected-skip-fix",
                "unexpected-interactive-fix",
            ] {
                assert!(
                    !stdout.contains(forbidden),
                    "predicate/forbidden output leaked {forbidden:?}: {stdout:?}"
                );
            }
        }
    }

    enum PtyRead {
        Bytes(Vec<u8>),
        Done,
        Error(std::io::Error),
    }

    struct PtyProcess {
        pid: rustix::process::Pid,
        writer: Option<File>,
        status: mpsc::Receiver<std::io::Result<ExitStatus>>,
        output: mpsc::Receiver<PtyRead>,
        collected: Vec<u8>,
        armed: bool,
    }

    impl PtyProcess {
        fn spawn(mut command: Command) -> Self {
            let master = rustix::pty::openpt(
                rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY,
            )
            .unwrap();
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
            let child_stdin = rustix::io::fcntl_dupfd_cloexec(&slave, 0).unwrap();
            let child_stdout = rustix::io::fcntl_dupfd_cloexec(&slave, 0).unwrap();
            let child_stderr = rustix::io::fcntl_dupfd_cloexec(&slave, 0).unwrap();
            command
                .stdin(Stdio::from(child_stdin))
                .stdout(Stdio::from(child_stdout))
                .stderr(Stdio::from(child_stderr));
            // SAFETY: the closure performs only async-signal-safe session and
            // controlling-terminal operations on an already-open descriptor.
            unsafe {
                command.pre_exec(move || {
                    rustix::process::setsid().map_err(std::io::Error::from)?;
                    rustix::process::ioctl_tiocsctty(&slave).map_err(std::io::Error::from)
                });
            }
            let child = ChildProcessGroup::new(command.spawn().unwrap());
            let pid = child.pid;

            let reader = rustix::io::fcntl_dupfd_cloexec(&master, 0).unwrap();
            let (output_sender, output) = mpsc::channel();
            thread::spawn(move || {
                let mut reader = File::from(reader);
                let mut buffer = [0_u8; 8192];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(length) => {
                            if output_sender
                                .send(PtyRead::Bytes(buffer[..length].to_vec()))
                                .is_err()
                            {
                                return;
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
                        Err(error) => {
                            let _ = output_sender.send(PtyRead::Error(error));
                            return;
                        }
                    }
                }
                let _ = output_sender.send(PtyRead::Done);
            });

            let (status_sender, status) = mpsc::channel();
            let child = child.disarm();
            thread::spawn(move || {
                let mut child = child;
                let _ = status_sender.send(child.wait());
            });

            Self {
                pid,
                writer: Some(File::from(master)),
                status,
                output,
                collected: Vec::new(),
                armed: true,
            }
        }

        fn wait_for(&mut self, needle: &[u8], timeout: Duration) {
            let deadline = Instant::now() + timeout;
            while !self
                .collected
                .windows(needle.len())
                .any(|window| window == needle)
            {
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(!remaining.is_zero(), "PTY output omitted {needle:?}");
                match self.output.recv_timeout(remaining) {
                    Ok(PtyRead::Bytes(bytes)) => self.collected.extend_from_slice(&bytes),
                    Ok(PtyRead::Done) => panic!("PTY closed before {needle:?}"),
                    Ok(PtyRead::Error(error)) => panic!("PTY read failed: {error}"),
                    Err(error) => panic!("PTY read timed out: {error}"),
                }
            }
        }

        fn send(&mut self, bytes: &[u8]) {
            let writer = self.writer.as_mut().unwrap();
            writer.write_all(bytes).unwrap();
            writer.flush().unwrap();
        }

        fn finish(mut self, timeout: Duration) -> (ExitStatus, Vec<u8>) {
            let status = match self.status.recv_timeout(timeout) {
                Ok(status) => status,
                Err(error) => {
                    kill_group_if_present(self.pid);
                    match self.status.recv_timeout(Duration::from_secs(3)) {
                        Ok(status) => status,
                        Err(_) => panic!("PTY Checksy did not exit after watchdog: {error}"),
                    }
                }
            };
            let status = status.unwrap();
            drop(self.writer.take());
            loop {
                match self.output.recv_timeout(Duration::from_secs(3)) {
                    Ok(PtyRead::Bytes(bytes)) => self.collected.extend_from_slice(&bytes),
                    Ok(PtyRead::Done) => break,
                    Ok(PtyRead::Error(error)) => panic!("PTY read failed: {error}"),
                    Err(error) => panic!("PTY output did not close: {error}"),
                }
            }
            self.armed = false;
            (status, std::mem::take(&mut self.collected))
        }
    }

    impl Drop for PtyProcess {
        fn drop(&mut self) {
            if self.armed {
                match rustix::process::kill_process_group(self.pid, rustix::process::Signal::Kill) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                    Err(_) => {}
                }
                let _ = self.status.recv_timeout(Duration::from_secs(3));
            }
        }
    }

    fn pty_file_command(root: &Path, fixture: &str) -> Command {
        file_command(root, fixture, &["--fix"])
    }

    fn pty_stdin_command(root: &Path, fixture: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command
            .current_dir(root)
            .args([
                "-c",
                "exec \"$CHECKSY_P0_BIN\" --stdin-config check --fix < \"$CHECKSY_P0_CONFIG\"",
            ])
            .env("CHECKSY_P0_BIN", env!("CARGO_BIN_EXE_checksy"))
            .env("CHECKSY_P0_CONFIG", root.join(fixture));
        command
    }

    fn set_interactive_environment(command: &mut Command, root: &Path) {
        command
            .env(
                "CHECKSY_P0_INTERACTIVE_TRACE",
                root.join("interactive-trace"),
            )
            .env(
                "CHECKSY_P0_INTERACTIVE_FIXED",
                root.join("interactive-fixed"),
            );
    }

    #[test]
    fn interactive_lifecycle_distinguishes_pty_headless_and_stdin() {
        let _serial = support::provisioning_test_guard();
        let corpus = corpus();
        let indexed = case(&corpus, "interactive-lifecycle");

        let pty_copy = writable_fixture_copy();
        let mut command = pty_file_command(pty_copy.path(), &indexed.fixture);
        set_interactive_environment(&mut command, pty_copy.path());
        let mut process = PtyProcess::spawn(command);
        process.wait_for(b"approval: ", Duration::from_secs(7));
        process.send(b"approve\n");
        let (status, output) = process.finish(Duration::from_secs(10));
        assert_eq!(status.code(), Some(0), "PTY output={output:?}");
        assert!(pty_copy.path().join("interactive-fixed").is_file());
        assert_eq!(
            fs::read_to_string(pty_copy.path().join("interactive-trace")).unwrap(),
            "interactive-check\ninteractive-fix\ninteractive-check\nlater-check\n"
        );
        let pty_output = String::from_utf8_lossy(&output);
        assert_ordered(
            &pty_output,
            &[
                "approval: ",
                "✅ interactive lifecycle",
                "✅ later interactive lifecycle rule",
                "😎 All rules validated",
            ],
        );

        let headless_copy = writable_fixture_copy();
        let mut command = file_command(headless_copy.path(), &indexed.fixture, &["--fix"]);
        set_interactive_environment(&mut command, headless_copy.path());
        let output = run_headless(command, None);
        assert_exit(&output, 3);
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("no usable controlling terminal is available"),
            "{}",
            output_context(&output)
        );
        assert!(!headless_copy.path().join("interactive-fixed").exists());
        assert_eq!(
            fs::read_to_string(headless_copy.path().join("interactive-trace")).unwrap(),
            "interactive-check\nlater-check\n"
        );
        let headless_stdout = String::from_utf8_lossy(&output.stdout);
        assert_ordered(
            &headless_stdout,
            &[
                "❌ interactive lifecycle",
                "✅ later interactive lifecycle rule",
                "😭 1 rules failed validation",
            ],
        );
        assert!(!headless_stdout.contains("approval: "));

        let stdin_copy = writable_fixture_copy();
        let mut command = pty_stdin_command(stdin_copy.path(), &indexed.fixture);
        set_interactive_environment(&mut command, stdin_copy.path());
        let process = PtyProcess::spawn(command);
        let (status, output) = process.finish(Duration::from_secs(10));
        assert_eq!(status.code(), Some(3), "PTY output={output:?}");
        let output = String::from_utf8_lossy(&output);
        assert!(output.contains("stdin configuration is always non-interactive"));
        assert!(!output.contains("approval: "));
        assert!(!stdin_copy.path().join("interactive-fixed").exists());
        assert_eq!(
            fs::read_to_string(stdin_copy.path().join("interactive-trace")).unwrap(),
            "interactive-check\nlater-check\n"
        );
        assert_ordered(
            &output,
            &[
                "❌ interactive lifecycle",
                "✅ later interactive lifecycle rule",
                "😭 1 rules failed validation",
            ],
        );
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
        child: Option<ChildProcessGroup>,
        release: File,
    }

    impl Holder {
        fn spawn(root: &Path, fixture: &str, stdin: bool, label: &str) -> Self {
            let ready_path = root.join(format!("{label}-ready.fifo"));
            let release_path = root.join(format!("{label}-release.fifo"));
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
            let document = fs::read(root.join(fixture)).unwrap();
            let mut command = if stdin {
                stdin_command(root, &["--fix", "--non-interactive"])
            } else {
                file_command(root, fixture, &["--fix", "--non-interactive"])
            };
            command
                .env("CHECKSY_P0_LOCK_TRACE", root.join(format!("{label}-trace")))
                .env("CHECKSY_P0_LOCK_FIXED", root.join(format!("{label}-fixed")))
                .env("CHECKSY_P0_LOCK_READY_FIFO", &ready_path)
                .env("CHECKSY_P0_LOCK_RELEASE_FIFO", &release_path)
                .env("CHECKSY_P0_LOCK_HOLDER", "1")
                .stdin(if stdin { Stdio::piped() } else { Stdio::null() })
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            // A fresh process group bounds holder cleanup without detaching
            // file and stdin modes from their ordinary test environment.
            command.process_group(0);
            let mut child = ChildProcessGroup::new(command.spawn().unwrap());
            if stdin {
                child
                    .child_mut()
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(&document)
                    .unwrap();
            }
            let holder = Self {
                child: Some(child),
                release,
            };
            wait_for_fifo_line(&ready);
            holder
        }

        fn finish(mut self) -> Output {
            writeln!(self.release, "release").unwrap();
            self.release.flush().unwrap();
            wait_output_bounded(self.child.take().unwrap().disarm(), COMMAND_TIMEOUT)
        }
    }

    impl Drop for Holder {
        fn drop(&mut self) {
            let _ = writeln!(self.release, "release");
            let _ = self.release.flush();
            drop(self.child.take());
        }
    }

    fn assert_lock_contention(output: &Output) {
        assert_exit(output, 4);
        assert!(output.stdout.is_empty(), "{}", output_context(output));
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "provisioning lock held: another checksy check --fix is already running for this user\n"
        );
    }

    #[test]
    fn file_and_stdin_provisioning_contend_on_the_same_semaphore() {
        let _serial = support::provisioning_test_guard();
        let corpus = corpus();
        let indexed = case(&corpus, "file-stdin-lock-contention");

        for holder_is_stdin in [false, true] {
            let copy = writable_fixture_copy();
            let label = if holder_is_stdin {
                "stdin-holder"
            } else {
                "file-holder"
            };
            let holder = Holder::spawn(copy.path(), &indexed.fixture, holder_is_stdin, label);
            let document = fs::read(copy.path().join(&indexed.fixture)).unwrap();
            let contender_is_stdin = !holder_is_stdin;
            let mut contender = if contender_is_stdin {
                stdin_command(copy.path(), &["--fix", "--non-interactive", "--no-fail"])
            } else {
                file_command(
                    copy.path(),
                    &indexed.fixture,
                    &["--fix", "--non-interactive", "--no-fail"],
                )
            };
            let contender_trace = copy.path().join("contender-trace");
            contender
                .env("CHECKSY_P0_LOCK_TRACE", &contender_trace)
                .env("CHECKSY_P0_LOCK_FIXED", copy.path().join("contender-fixed"))
                .env("CHECKSY_P0_LOCK_HOLDER", "0");
            let output = run_headless(contender, contender_is_stdin.then_some(document.as_slice()));
            assert_lock_contention(&output);
            assert!(!contender_trace.exists());

            let output = holder.finish();
            assert_exit(&output, 0);
            assert!(
                String::from_utf8_lossy(&output.stdout).contains("😎 All rules validated"),
                "{}",
                output_context(&output)
            );
            assert_eq!(
                fs::read_to_string(copy.path().join(format!("{label}-trace"))).unwrap(),
                "holder-check\nholder-fix\nholder-check\n"
            );
        }
    }

    fn hold_advisory_lock(path: &Path) -> File {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive).unwrap();
        file
    }

    fn assert_lock_reacquirable(path: &Path) {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
            .unwrap_or_else(|error| panic!("{} remained locked: {error}", path.display()));
    }

    fn tree_helper_command(role: &str, directory: &Path) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--ignored",
                "--exact",
                "p0_acceptance_tree_helper",
                "--nocapture",
            ])
            .env("CHECKSY_P0_TREE_ROLE", role)
            .env("CHECKSY_P0_TREE_DIR", directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        command
    }

    fn wait_for_helper_ready(child: &mut Child) {
        let stdout = child.stdout.take().expect("helper readiness pipe");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = reader.read_line(&mut line).unwrap();
            assert_ne!(bytes, 0, "tree helper exited before readiness");
            if line.trim() == "CHECKSY_P0_TREE_READY" {
                return;
            }
        }
    }

    pub(super) fn run_tree_helper() {
        // SAFETY: this isolated helper deliberately models commands that
        // resist the runner's TERM phase. The runner must escalate to KILL.
        unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_IGN);
        }
        let role = std::env::var("CHECKSY_P0_TREE_ROLE").unwrap();
        let directory = PathBuf::from(std::env::var_os("CHECKSY_P0_TREE_DIR").unwrap());
        let _lock = hold_advisory_lock(&directory.join(format!("{role}.lock")));

        match role.as_str() {
            "grandchild" => {
                println!("CHECKSY_P0_TREE_READY");
                std::io::stdout().flush().unwrap();
                loop {
                    thread::park();
                }
            }
            "child" => {
                let mut grandchild = tree_helper_command("grandchild", &directory)
                    .spawn()
                    .unwrap();
                wait_for_helper_ready(&mut grandchild);
                println!("CHECKSY_P0_TREE_READY");
                std::io::stdout().flush().unwrap();
                let _grandchild = grandchild;
                loop {
                    thread::park();
                }
            }
            "leader" => {
                let process_group = rustix::process::getpgrp();
                let (cancel, receiver) = mpsc::channel::<()>();
                thread::spawn(move || {
                    if receiver.recv_timeout(Duration::from_secs(12)).is_err() {
                        kill_group_if_present(process_group);
                    }
                });
                let mut child = tree_helper_command("child", &directory).spawn().unwrap();
                wait_for_helper_ready(&mut child);
                let socket_path =
                    PathBuf::from(std::env::var_os("CHECKSY_P0_TREE_READY_SOCKET").unwrap());
                UnixDatagram::unbound()
                    .unwrap()
                    .send_to(b"ready", socket_path)
                    .unwrap();
                let _cancel = cancel;
                let _child = child;
                loop {
                    thread::park();
                }
            }
            other => panic!("unknown P0 tree helper role {other:?}"),
        }
    }

    #[test]
    fn predicate_timeout_cleans_managed_descendants() {
        let _serial = support::provisioning_test_guard();
        let corpus = corpus();
        let indexed = case(&corpus, "predicate-tree-timeout");
        let copy = writable_fixture_copy();
        let ready_path = copy.path().join("tree-ready.sock");
        let ready = UnixDatagram::bind(&ready_path).unwrap();
        ready
            .set_read_timeout(Some(Duration::from_secs(7)))
            .unwrap();
        let unexpected_check = copy.path().join("unexpected-check");
        let unexpected_later = copy.path().join("unexpected-later");

        let mut command = file_command(
            copy.path(),
            &indexed.fixture,
            &["--fix", "--non-interactive", "--no-fail"],
        );
        command
            .env("CHECKSY_P0_TREE_HELPER", std::env::current_exe().unwrap())
            .env("CHECKSY_P0_TREE_ROLE", "leader")
            .env("CHECKSY_P0_TREE_DIR", copy.path())
            .env("CHECKSY_P0_TREE_READY_SOCKET", &ready_path)
            .env("CHECKSY_P0_TREE_UNEXPECTED_CHECK", &unexpected_check)
            .env("CHECKSY_P0_TREE_UNEXPECTED_LATER", &unexpected_later)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        command.process_group(0);
        let child = ChildProcessGroup::new(command.spawn().unwrap());
        let started = Instant::now();
        let mut message = [0_u8; 16];
        ready
            .recv(&mut message)
            .unwrap_or_else(|error| panic!("managed tree did not become ready: {error}"));
        let output = wait_output_bounded(child.disarm(), Duration::from_secs(15));
        assert_exit(&output, 2);
        assert!(started.elapsed() < Duration::from_secs(12));
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            combined.contains("predicate tree retained stdout"),
            "{combined}"
        );
        assert!(
            combined.contains("predicate tree retained stderr"),
            "{combined}"
        );
        assert!(
            combined.to_ascii_lowercase().contains("timed out"),
            "{combined}"
        );
        assert!(!unexpected_check.exists());
        assert!(!unexpected_later.exists());
        for role in ["leader", "child", "grandchild"] {
            assert_lock_reacquirable(&copy.path().join(format!("{role}.lock")));
        }
    }

    #[test]
    fn invalid_configuration_precedes_commands_and_lock() {
        let _serial = support::provisioning_test_guard();
        let corpus = corpus();
        let invalid = case(&corpus, "invalid-preflight");

        for stdin in [false, true] {
            let copy = writable_fixture_copy();
            let marker = copy.path().join("invalid-marker");
            let document = fs::read(copy.path().join(&invalid.fixture)).unwrap();
            let mut command = if stdin {
                stdin_command(copy.path(), &["--fix", "--no-fail"])
            } else {
                file_command(copy.path(), &invalid.fixture, &["--fix", "--no-fail"])
            };
            command.env("CHECKSY_P0_INVALID_MARKER", &marker);
            let output = run_headless(command, stdin.then_some(document.as_slice()));
            assert_exit(&output, 2);
            assert!(!marker.exists());
            assert!(String::from_utf8_lossy(&output.stderr).contains("skip-if"));
        }

        let lock_case = case(&corpus, "file-stdin-lock-contention");
        let copy = writable_fixture_copy();
        let holder = Holder::spawn(copy.path(), &lock_case.fixture, false, "invalid-holder");
        let marker = copy.path().join("invalid-marker-under-lock");
        let mut command = file_command(copy.path(), &invalid.fixture, &["--fix", "--no-fail"]);
        command.env("CHECKSY_P0_INVALID_MARKER", &marker);
        let output = run_headless(command, None);
        assert_exit(&output, 2);
        assert!(!marker.exists());
        assert!(String::from_utf8_lossy(&output.stderr).contains("skip-if"));
        let output = holder.finish();
        assert_exit(&output, 0);
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
#[ignore = "isolated workload invoked by the P0 acceptance test"]
fn p0_acceptance_tree_helper() {
    unix::run_tree_helper();
}
