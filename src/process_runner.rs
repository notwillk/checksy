use std::fmt;
use std::io;
use std::process::{Command, ExitStatus};
use std::time::Duration;

pub(crate) const CAPTURE_LIMIT_BYTES: usize = 1024 * 1024;
pub(crate) const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(15 * 60);
pub(crate) const DEFAULT_GIT_RESOLVE_TIMEOUT: Duration = Duration::from_secs(60);
pub(crate) const DEFAULT_GIT_FETCH_TIMEOUT: Duration = Duration::from_secs(5 * 60);
pub(crate) const DEFAULT_TERM_GRACE: Duration = Duration::from_secs(5);
const MAX_PROCESS_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
const MAX_TERM_GRACE: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProcessLimits {
    pub(crate) timeout: Duration,
    pub(crate) term_grace: Duration,
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_COMMAND_TIMEOUT,
            term_grace: DEFAULT_TERM_GRACE,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CapturedOutput {
    /// Retained bytes: the complete stream when untruncated, otherwise equal
    /// head and tail halves of the stream.
    pub(crate) bytes: Vec<u8>,
    pub(crate) original_bytes: u64,
    pub(crate) truncated: bool,
}

impl CapturedOutput {
    fn empty() -> Self {
        Self {
            bytes: Vec::new(),
            original_bytes: 0,
            truncated: false,
        }
    }

    pub(crate) fn render_lossy(&self) -> String {
        if !self.truncated {
            return String::from_utf8_lossy(&self.bytes).into_owned();
        }

        let split = self.bytes.len() / 2;
        let head = String::from_utf8_lossy(&self.bytes[..split]);
        let tail = String::from_utf8_lossy(&self.bytes[split..]);
        format!(
            "{head}\n... {} bytes omitted from bounded process output ...\n{tail}",
            self.original_bytes.saturating_sub(self.bytes.len() as u64)
        )
    }
}

#[derive(Debug)]
pub(crate) struct CompletedProcess {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: CapturedOutput,
    pub(crate) stderr: CapturedOutput,
}

#[derive(Clone, Debug)]
pub(crate) struct PartialProcessOutput {
    pub(crate) status: Option<ExitStatus>,
    pub(crate) stdout: CapturedOutput,
    pub(crate) stderr: CapturedOutput,
}

impl PartialProcessOutput {
    fn empty() -> Self {
        Self {
            status: None,
            stdout: CapturedOutput::empty(),
            stderr: CapturedOutput::empty(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ProcessError {
    Spawn(io::Error),
    Supervision {
        source: io::Error,
        output: PartialProcessOutput,
    },
    TimedOut {
        timeout: Duration,
        output: PartialProcessOutput,
    },
    #[allow(dead_code)] // Constructed by `run` on unsupported target builds.
    UnsupportedPlatform,
}

impl ProcessError {
    pub(crate) fn output(&self) -> Option<&PartialProcessOutput> {
        match self {
            Self::Supervision { output, .. } | Self::TimedOut { output, .. } => Some(output),
            Self::Spawn(_) | Self::UnsupportedPlatform => None,
        }
    }
}

impl fmt::Display for ProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "failed to spawn process: {error}"),
            Self::Supervision { source, output } => {
                write!(formatter, "failed to supervise process: {source}")?;
                if let Some(status) = output.status {
                    write!(formatter, " (leader status: {status})")?;
                }
                Ok(())
            }
            Self::TimedOut { timeout, output } => {
                write!(
                    formatter,
                    "process timed out after {}ms",
                    timeout.as_millis()
                )?;
                if let Some(status) = output.status {
                    write!(formatter, " (leader status: {status})")?;
                }
                Ok(())
            }
            Self::UnsupportedPlatform => {
                formatter.write_str("process supervision is supported only on Linux and macOS")
            }
        }
    }
}

impl std::error::Error for ProcessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn(error) | Self::Supervision { source: error, .. } => Some(error),
            Self::TimedOut { .. } | Self::UnsupportedPlatform => None,
        }
    }
}

/// Run a configured command under Checksy's bounded process supervisor.
///
/// The caller owns arguments, environment, and working directory. This
/// function deliberately overrides all three standard streams: stdin is
/// `/dev/null`, while stdout and stderr are independently drained pipes.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn run(
    command: Command,
    limits: ProcessLimits,
) -> Result<CompletedProcess, ProcessError> {
    supported::run(command, limits)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) fn run(
    _command: Command,
    _limits: ProcessLimits,
) -> Result<CompletedProcess, ProcessError> {
    Err(ProcessError::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod supported {
    use super::{
        CapturedOutput, CompletedProcess, PartialProcessOutput, ProcessError, ProcessLimits,
        CAPTURE_LIMIT_BYTES,
    };
    use rustix::fs::{fcntl_getfl, fcntl_setfl, OFlags};
    use rustix::io::{self as rustix_io, Errno, PollFd, PollFlags};
    use rustix::process::{kill_process_group, Pid, Signal};
    use std::collections::VecDeque;
    use std::io;
    use std::os::unix::process::CommandExt;
    use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
    use std::time::{Duration, Instant};

    const READ_BUFFER_BYTES: usize = 32 * 1024;
    const MAX_DRAIN_BYTES_PER_PASS: usize = 256 * 1024;
    const SUPERVISION_TICK: Duration = Duration::from_millis(25);
    const KILL_REAP_LIMIT: Duration = Duration::from_secs(1);

    pub(super) fn run(
        mut command: Command,
        limits: ProcessLimits,
    ) -> Result<CompletedProcess, ProcessError> {
        validate_limits(limits)?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);

        let child = command.spawn().map_err(ProcessError::Spawn)?;
        let mut child = ArmedChild::new(child);
        let mut stdout = child
            .child_mut()
            .stdout
            .take()
            .ok_or_else(|| supervision_error(&child, "spawned process has no stdout pipe"))?;
        let mut stderr = child
            .child_mut()
            .stderr
            .take()
            .ok_or_else(|| supervision_error(&child, "spawned process has no stderr pipe"))?;

        set_nonblocking(&stdout).map_err(|error| ProcessError::Supervision {
            source: error,
            output: PartialProcessOutput::empty(),
        })?;
        set_nonblocking(&stderr).map_err(|error| ProcessError::Supervision {
            source: error,
            output: PartialProcessOutput::empty(),
        })?;

        let started_at = Instant::now();
        let timeout_at = started_at.checked_add(limits.timeout).unwrap_or(started_at);
        let mut stdout_capture = CaptureBuffer::new(CAPTURE_LIMIT_BYTES);
        let mut stderr_capture = CaptureBuffer::new(CAPTURE_LIMIT_BYTES);
        let mut stdout_open = true;
        let mut stderr_open = true;
        let mut timed_out = false;
        let mut term_deadline = None;
        let mut kill_deadline = None;
        let mut group_killed = false;

        loop {
            // Reap only after the final group KILL. Before then, an exited but
            // unreaped leader keeps its PID/PGID reserved, so a later timeout
            // signal cannot target an unrelated process group after numeric
            // ID reuse. Ordinary completion is reaped below only when both
            // capture pipes have reached EOF and no later signal is needed.
            if group_killed && child.status().is_none() {
                match child.child_mut().try_wait() {
                    Ok(Some(status)) => child.set_status(status),
                    Ok(None) => {}
                    Err(error) => {
                        return Err(supervision_with_output(
                            error,
                            &child,
                            &stdout_capture,
                            &stderr_capture,
                        ));
                    }
                }
            }

            if !timed_out && Instant::now() >= timeout_at {
                timed_out = true;
                signal_group(child.process_group(), Signal::Term).map_err(|error| {
                    supervision_with_output(error, &child, &stdout_capture, &stderr_capture)
                })?;
                let now = Instant::now();
                term_deadline = Some(now.checked_add(limits.term_grace).unwrap_or(now));
            }

            if timed_out && kill_deadline.is_none() {
                let deadline = term_deadline.expect("a timed-out process has a TERM deadline");
                if Instant::now() >= deadline {
                    signal_group(child.process_group(), Signal::Kill).map_err(|error| {
                        supervision_with_output(error, &child, &stdout_capture, &stderr_capture)
                    })?;
                    let now = Instant::now();
                    kill_deadline = Some(now.checked_add(KILL_REAP_LIMIT).unwrap_or(now));
                    group_killed = true;
                    child.mark_group_killed();
                }
            }

            if !timed_out && !stdout_open && !stderr_open {
                match child.child_mut().try_wait() {
                    Ok(Some(status)) => {
                        child.set_status(status);
                        child.disarm();
                        return Ok(CompletedProcess {
                            status,
                            stdout: stdout_capture.finish(),
                            stderr: stderr_capture.finish(),
                        });
                    }
                    Ok(None) => {}
                    Err(error) => {
                        return Err(supervision_with_output(
                            error,
                            &child,
                            &stdout_capture,
                            &stderr_capture,
                        ));
                    }
                }
            }

            if timed_out && group_killed && child.status().is_some() && !stdout_open && !stderr_open
            {
                child.disarm();
                return Err(timeout_with_output(
                    limits.timeout,
                    &child,
                    &stdout_capture,
                    &stderr_capture,
                ));
            }

            if let Some(deadline) = kill_deadline {
                if Instant::now() >= deadline {
                    // Close any inherited pipe descriptors rather than permit
                    // an uncooperative process tree to extend cleanup forever.
                    return Err(timeout_with_output(
                        limits.timeout,
                        &child,
                        &stdout_capture,
                        &stderr_capture,
                    ));
                }
            }

            let wake_at = next_wake(timed_out, timeout_at, term_deadline, kill_deadline);
            poll_streams(
                stdout_open.then_some(&stdout),
                stderr_open.then_some(&stderr),
                wake_at,
            )
            .map_err(|error| {
                supervision_with_output(error, &child, &stdout_capture, &stderr_capture)
            })?;

            if stdout_open {
                stdout_open = drain_stream(&mut stdout, &mut stdout_capture).map_err(|error| {
                    supervision_with_output(error, &child, &stdout_capture, &stderr_capture)
                })?;
            }
            if stderr_open {
                stderr_open = drain_stream(&mut stderr, &mut stderr_capture).map_err(|error| {
                    supervision_with_output(error, &child, &stdout_capture, &stderr_capture)
                })?;
            }
        }
    }

    fn validate_limits(limits: ProcessLimits) -> Result<(), ProcessError> {
        let invalid = if limits.timeout.is_zero() {
            Some("process timeout must be greater than zero")
        } else if limits.term_grace.is_zero() {
            Some("process termination grace must be greater than zero")
        } else if limits.timeout > super::MAX_PROCESS_TIMEOUT {
            Some("process timeout exceeds the 2h hard maximum")
        } else if limits.term_grace > super::MAX_TERM_GRACE {
            Some("process termination grace exceeds the 30s hard maximum")
        } else {
            None
        };

        match invalid {
            Some(message) => Err(ProcessError::Supervision {
                source: io::Error::new(io::ErrorKind::InvalidInput, message),
                output: PartialProcessOutput::empty(),
            }),
            None => Ok(()),
        }
    }

    fn set_nonblocking<Fd: rustix::fd::AsFd>(fd: &Fd) -> Result<(), io::Error> {
        let flags = fcntl_getfl(fd).map_err(os_error)?;
        fcntl_setfl(fd, flags | OFlags::NONBLOCK).map_err(os_error)
    }

    fn poll_streams(
        stdout: Option<&ChildStdout>,
        stderr: Option<&ChildStderr>,
        wake_at: Instant,
    ) -> Result<(), io::Error> {
        let mut poll_fds = Vec::with_capacity(2);
        if let Some(stdout) = stdout {
            poll_fds.push(PollFd::new(
                stdout,
                PollFlags::IN | PollFlags::HUP | PollFlags::ERR,
            ));
        }
        if let Some(stderr) = stderr {
            poll_fds.push(PollFd::new(
                stderr,
                PollFlags::IN | PollFlags::HUP | PollFlags::ERR,
            ));
        }

        loop {
            let timeout_ms = poll_timeout_ms(wake_at);
            match rustix_io::poll(&mut poll_fds, timeout_ms) {
                Ok(_) => return Ok(()),
                Err(Errno::INTR) if Instant::now() < wake_at => continue,
                Err(Errno::INTR) => return Ok(()),
                Err(error) => return Err(os_error(error)),
            }
        }
    }

    fn poll_timeout_ms(wake_at: Instant) -> i32 {
        let remaining = wake_at.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return 0;
        }
        let millis = remaining.as_millis().max(1);
        i32::try_from(millis).unwrap_or(i32::MAX)
    }

    fn next_wake(
        timed_out: bool,
        timeout_at: Instant,
        term_deadline: Option<Instant>,
        kill_deadline: Option<Instant>,
    ) -> Instant {
        let now = Instant::now();
        let tick = now.checked_add(SUPERVISION_TICK).unwrap_or(now);
        let phase_deadline = if !timed_out {
            timeout_at
        } else if let Some(deadline) = kill_deadline {
            deadline
        } else {
            term_deadline.expect("a timed-out process has a TERM deadline")
        };
        tick.min(phase_deadline)
    }

    fn drain_stream<Fd: rustix::fd::AsFd>(
        fd: &mut Fd,
        capture: &mut CaptureBuffer,
    ) -> Result<bool, io::Error> {
        let mut buffer = [0_u8; READ_BUFFER_BYTES];
        let mut drained = 0_usize;
        loop {
            match rustix_io::read(&*fd, &mut buffer) {
                Ok(0) => return Ok(false),
                Ok(read) => {
                    capture.push(&buffer[..read])?;
                    drained = drained.saturating_add(read);
                    if drained >= MAX_DRAIN_BYTES_PER_PASS {
                        // Yield to the other stream and the monotonic deadline
                        // even when this producer can keep its pipe perpetually
                        // readable.
                        return Ok(true);
                    }
                }
                Err(Errno::INTR) => continue,
                // POSIX permits EAGAIN and EWOULDBLOCK to be distinct, but
                // both supported targets expose the latter as an alias of the
                // former through rustix.
                Err(Errno::AGAIN) => return Ok(true),
                Err(error) => return Err(os_error(error)),
            }
        }
    }

    fn signal_group(process_group: Pid, signal: Signal) -> Result<(), io::Error> {
        match kill_process_group(process_group, signal) {
            Ok(()) | Err(Errno::SRCH) => Ok(()),
            Err(error) => Err(os_error(error)),
        }
    }

    fn os_error(error: Errno) -> io::Error {
        io::Error::from_raw_os_error(error.raw_os_error())
    }

    fn supervision_error(child: &ArmedChild, message: &str) -> ProcessError {
        ProcessError::Supervision {
            source: io::Error::other(message),
            output: PartialProcessOutput {
                status: child.status(),
                stdout: CapturedOutput::empty(),
                stderr: CapturedOutput::empty(),
            },
        }
    }

    fn supervision_with_output(
        source: io::Error,
        child: &ArmedChild,
        stdout: &CaptureBuffer,
        stderr: &CaptureBuffer,
    ) -> ProcessError {
        ProcessError::Supervision {
            source,
            output: partial_output(child, stdout, stderr),
        }
    }

    fn timeout_with_output(
        timeout: Duration,
        child: &ArmedChild,
        stdout: &CaptureBuffer,
        stderr: &CaptureBuffer,
    ) -> ProcessError {
        ProcessError::TimedOut {
            timeout,
            output: partial_output(child, stdout, stderr),
        }
    }

    fn partial_output(
        child: &ArmedChild,
        stdout: &CaptureBuffer,
        stderr: &CaptureBuffer,
    ) -> PartialProcessOutput {
        PartialProcessOutput {
            status: child.status(),
            stdout: stdout.snapshot(),
            stderr: stderr.snapshot(),
        }
    }

    struct ArmedChild {
        child: Child,
        process_group: Pid,
        status: Option<ExitStatus>,
        armed: bool,
        group_killed: bool,
    }

    impl ArmedChild {
        fn new(child: Child) -> Self {
            let process_group = Pid::from_child(&child);
            Self {
                child,
                process_group,
                status: None,
                armed: true,
                group_killed: false,
            }
        }

        fn child_mut(&mut self) -> &mut Child {
            &mut self.child
        }

        fn process_group(&self) -> Pid {
            self.process_group
        }

        fn status(&self) -> Option<ExitStatus> {
            self.status
        }

        fn set_status(&mut self, status: ExitStatus) {
            self.status = Some(status);
        }

        fn disarm(&mut self) {
            self.armed = false;
        }

        fn mark_group_killed(&mut self) {
            self.group_killed = true;
        }
    }

    impl Drop for ArmedChild {
        fn drop(&mut self) {
            if !self.armed {
                return;
            }

            if !self.group_killed {
                let _ = signal_group(self.process_group, Signal::Kill);
            }
            if self.status.is_some() {
                return;
            }

            let started = Instant::now();
            let deadline = started.checked_add(KILL_REAP_LIMIT).unwrap_or(started);
            loop {
                match self.child.try_wait() {
                    Ok(Some(status)) => {
                        self.status = Some(status);
                        return;
                    }
                    Ok(None) if Instant::now() < deadline => {
                        let wake = Instant::now()
                            .checked_add(SUPERVISION_TICK)
                            .unwrap_or(deadline)
                            .min(deadline);
                        let _ = rustix_io::poll(&mut [], poll_timeout_ms(wake));
                    }
                    Ok(None) | Err(_) => return,
                }
            }
        }
    }

    #[derive(Clone, Debug)]
    struct CaptureBuffer {
        head: Vec<u8>,
        tail: VecDeque<u8>,
        head_limit: usize,
        tail_limit: usize,
        original_bytes: u64,
    }

    impl CaptureBuffer {
        fn new(limit: usize) -> Self {
            let head_limit = limit / 2;
            Self {
                head: Vec::with_capacity(head_limit),
                tail: VecDeque::with_capacity(limit - head_limit),
                head_limit,
                tail_limit: limit - head_limit,
                original_bytes: 0,
            }
        }

        fn push(&mut self, mut bytes: &[u8]) -> Result<(), io::Error> {
            let added = u64::try_from(bytes.len())
                .map_err(|_| io::Error::other("process output byte count overflow"))?;
            self.original_bytes = self
                .original_bytes
                .checked_add(added)
                .ok_or_else(|| io::Error::other("process output byte count overflow"))?;

            let head_remaining = self.head_limit - self.head.len();
            let head_bytes = head_remaining.min(bytes.len());
            self.head.extend_from_slice(&bytes[..head_bytes]);
            bytes = &bytes[head_bytes..];

            if bytes.len() >= self.tail_limit {
                self.tail.clear();
                self.tail
                    .extend(&bytes[bytes.len().saturating_sub(self.tail_limit)..]);
            } else {
                let excess = self
                    .tail
                    .len()
                    .saturating_add(bytes.len())
                    .saturating_sub(self.tail_limit);
                self.tail.drain(..excess);
                self.tail.extend(bytes);
            }
            Ok(())
        }

        fn snapshot(&self) -> CapturedOutput {
            let mut bytes = Vec::with_capacity(self.head.len() + self.tail.len());
            bytes.extend_from_slice(&self.head);
            bytes.extend(self.tail.iter().copied());
            CapturedOutput {
                bytes,
                original_bytes: self.original_bytes,
                truncated: self.original_bytes > (self.head_limit + self.tail_limit) as u64,
            }
        }

        fn finish(self) -> CapturedOutput {
            self.snapshot()
        }
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::{
        run, ProcessError, ProcessLimits, CAPTURE_LIMIT_BYTES, MAX_PROCESS_TIMEOUT, MAX_TERM_GRACE,
    };
    use std::process::{Command, Stdio};
    use std::time::Duration;
    use tempfile::tempdir;

    fn command(script: &str) -> Command {
        let mut command = Command::new("bash");
        command.arg("-c").arg(script);
        command
    }

    fn short_limits() -> ProcessLimits {
        ProcessLimits {
            timeout: Duration::from_secs(5),
            term_grace: Duration::from_millis(25),
        }
    }

    #[test]
    fn captures_successful_process_output() {
        let completed = run(command("printf stdout; printf stderr >&2"), short_limits()).unwrap();

        assert!(completed.status.success());
        assert_eq!(completed.stdout.bytes, b"stdout");
        assert_eq!(completed.stderr.bytes, b"stderr");
        assert_eq!(completed.stdout.original_bytes, 6);
        assert!(!completed.stdout.truncated);
    }

    #[test]
    fn returns_nonzero_exit_as_completed_process() {
        let completed = run(
            command("printf before; printf problem >&2; exit 7"),
            short_limits(),
        )
        .unwrap();

        assert_eq!(completed.status.code(), Some(7));
        assert_eq!(completed.stdout.bytes, b"before");
        assert_eq!(completed.stderr.bytes, b"problem");
    }

    #[test]
    fn reports_spawn_failure_separately() {
        let command = Command::new("/path/that/cannot/exist/checksy-test");
        assert!(matches!(
            run(command, short_limits()),
            Err(ProcessError::Spawn(_))
        ));
    }

    #[test]
    fn forces_stdin_to_null() {
        let mut command = command("if read -r line; then exit 9; else printf eof; fi");
        // Prove the supervisor overrides an explicitly configured pipe. If it
        // did not, `read` would wait on the still-open parent end until the
        // test's process timeout.
        command.stdin(Stdio::piped());
        let completed = run(command, short_limits()).unwrap();

        assert!(completed.status.success());
        assert_eq!(completed.stdout.bytes, b"eof");
    }

    #[test]
    fn invalid_limits_are_rejected_before_spawn() {
        for limits in [
            ProcessLimits {
                timeout: Duration::ZERO,
                term_grace: Duration::from_millis(1),
            },
            ProcessLimits {
                timeout: Duration::from_millis(1),
                term_grace: Duration::ZERO,
            },
            ProcessLimits {
                timeout: MAX_PROCESS_TIMEOUT + Duration::from_millis(1),
                term_grace: Duration::from_millis(1),
            },
            ProcessLimits {
                timeout: Duration::from_millis(1),
                term_grace: MAX_TERM_GRACE + Duration::from_millis(1),
            },
        ] {
            let directory = tempdir().unwrap();
            let marker = directory.path().join("spawned");
            let script = format!("touch '{}'", marker.display());

            assert!(matches!(
                run(command(&script), limits),
                Err(ProcessError::Supervision { .. })
            ));
            assert!(!marker.exists());
        }
    }

    #[test]
    fn timeout_preserves_output_emitted_before_termination() {
        let limits = ProcessLimits {
            timeout: Duration::from_millis(250),
            term_grace: Duration::from_millis(25),
        };
        let error = run(
            command("trap '' TERM; printf before; printf warning >&2; while :; do sleep 1; done"),
            limits,
        )
        .unwrap_err();

        match error {
            ProcessError::TimedOut { timeout, output } => {
                assert_eq!(timeout, limits.timeout);
                assert_eq!(output.stdout.bytes, b"before");
                assert_eq!(output.stderr.bytes, b"warning");
                assert!(output.status.is_some());
            }
            other => panic!("expected timeout, got {other:?}"),
        }
    }

    #[test]
    fn continuous_output_cannot_starve_the_timeout_deadline() {
        let limits = ProcessLimits {
            timeout: Duration::from_millis(100),
            term_grace: Duration::from_millis(25),
        };
        let error = run(
            command("printf stderr-marker >&2; while :; do printf 0123456789; done"),
            limits,
        )
        .unwrap_err();

        match error {
            ProcessError::TimedOut { output, .. } => {
                assert!(output.stdout.original_bytes > 0);
                assert_eq!(output.stderr.bytes, b"stderr-marker");
            }
            other => panic!("expected timeout, got {other:?}"),
        }
    }

    #[test]
    fn exact_capture_limit_is_not_truncated() {
        let script = format!(
            "printf H; head -c {} /dev/zero; printf T",
            CAPTURE_LIMIT_BYTES - 2
        );
        let completed = run(command(&script), short_limits()).unwrap();

        assert_eq!(completed.stdout.bytes.len(), CAPTURE_LIMIT_BYTES);
        assert_eq!(completed.stdout.original_bytes, CAPTURE_LIMIT_BYTES as u64);
        assert!(!completed.stdout.truncated);
        assert_eq!(completed.stdout.bytes.first(), Some(&b'H'));
        assert_eq!(completed.stdout.bytes.last(), Some(&b'T'));
    }

    #[test]
    fn maximum_plus_one_retains_equal_head_and_tail_halves() {
        let script = format!(
            "printf H; head -c {} /dev/zero; printf T",
            CAPTURE_LIMIT_BYTES - 1
        );
        let completed = run(command(&script), short_limits()).unwrap();

        assert_eq!(completed.stdout.bytes.len(), CAPTURE_LIMIT_BYTES);
        assert_eq!(
            completed.stdout.original_bytes,
            (CAPTURE_LIMIT_BYTES + 1) as u64
        );
        assert!(completed.stdout.truncated);
        assert_eq!(completed.stdout.bytes.first(), Some(&b'H'));
        assert_eq!(completed.stdout.bytes.last(), Some(&b'T'));
        assert_eq!(
            &completed.stdout.bytes[..CAPTURE_LIMIT_BYTES / 2],
            &[vec![b'H'], vec![0; CAPTURE_LIMIT_BYTES / 2 - 1]].concat()
        );
        assert_eq!(
            &completed.stdout.bytes[CAPTURE_LIMIT_BYTES / 2..],
            &[vec![0; CAPTURE_LIMIT_BYTES / 2 - 1], vec![b'T']].concat()
        );
    }

    #[test]
    fn drains_stdout_and_stderr_independently() {
        let script = "(head -c 200000 /dev/zero >&1) & head -c 300000 /dev/zero >&2; wait";
        let completed = run(command(script), short_limits()).unwrap();

        assert!(completed.status.success());
        assert_eq!(completed.stdout.original_bytes, 200_000);
        assert_eq!(completed.stderr.original_bytes, 300_000);
        assert_eq!(completed.stdout.bytes.len(), 200_000);
        assert_eq!(completed.stderr.bytes.len(), 300_000);
    }
}
