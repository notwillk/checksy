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

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
fn run_observed(
    command: Command,
    limits: ProcessLimits,
    observer: impl FnMut(ProcessTestEvent),
) -> Result<CompletedProcess, ProcessError> {
    supported::run_observed(command, limits, observer)
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessTestEvent {
    Spawned { process_group: u32 },
    TermSent,
    KillSent,
    LeaderReaped,
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
    #[cfg(test)]
    use super::ProcessTestEvent;
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
        command: Command,
        limits: ProcessLimits,
    ) -> Result<CompletedProcess, ProcessError> {
        run_impl(command, limits, &mut NoopObserver)
    }

    #[cfg(test)]
    pub(super) fn run_observed(
        command: Command,
        limits: ProcessLimits,
        observer: impl FnMut(ProcessTestEvent),
    ) -> Result<CompletedProcess, ProcessError> {
        run_impl(command, limits, &mut CallbackObserver(observer))
    }

    trait ProcessObserver {
        fn spawned(&mut self, _process_group: u32) {}
        fn term_sent(&mut self) {}
        fn kill_sent(&mut self) {}
        fn leader_reaped(&mut self) {}
    }

    struct NoopObserver;

    impl ProcessObserver for NoopObserver {}

    #[cfg(test)]
    struct CallbackObserver<F>(F);

    #[cfg(test)]
    impl<F: FnMut(ProcessTestEvent)> ProcessObserver for CallbackObserver<F> {
        fn spawned(&mut self, process_group: u32) {
            (self.0)(ProcessTestEvent::Spawned { process_group });
        }

        fn term_sent(&mut self) {
            (self.0)(ProcessTestEvent::TermSent);
        }

        fn kill_sent(&mut self) {
            (self.0)(ProcessTestEvent::KillSent);
        }

        fn leader_reaped(&mut self) {
            (self.0)(ProcessTestEvent::LeaderReaped);
        }
    }

    fn run_impl<O: ProcessObserver>(
        mut command: Command,
        limits: ProcessLimits,
        observer: &mut O,
    ) -> Result<CompletedProcess, ProcessError> {
        validate_limits(limits)?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);

        let child = command.spawn().map_err(ProcessError::Spawn)?;
        let mut child = ArmedChild::new(child);
        observer.spawned(stable_pid(child.process_group()));
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
                    Ok(Some(status)) => {
                        child.set_status(status);
                        observer.leader_reaped();
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

            if !timed_out && Instant::now() >= timeout_at {
                timed_out = true;
                signal_group(child.process_group(), Signal::Term).map_err(|error| {
                    supervision_with_output(error, &child, &stdout_capture, &stderr_capture)
                })?;
                observer.term_sent();
                let now = Instant::now();
                term_deadline = Some(now.checked_add(limits.term_grace).unwrap_or(now));
            }

            if timed_out && kill_deadline.is_none() {
                let deadline = term_deadline.expect("a timed-out process has a TERM deadline");
                if Instant::now() >= deadline {
                    signal_group(child.process_group(), Signal::Kill).map_err(|error| {
                        supervision_with_output(error, &child, &stdout_capture, &stderr_capture)
                    })?;
                    observer.kill_sent();
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
                        observer.leader_reaped();
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

    fn stable_pid(pid: Pid) -> u32 {
        #[cfg(target_os = "linux")]
        {
            Pid::as_raw(Some(pid))
        }
        #[cfg(target_os = "macos")]
        {
            u32::try_from(Pid::as_raw(Some(pid))).expect("child process ID must fit in u32")
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
        run, run_observed, ProcessError, ProcessLimits, ProcessTestEvent, CAPTURE_LIMIT_BYTES,
        MAX_PROCESS_TIMEOUT, MAX_TERM_GRACE,
    };
    use crate::state_lock::{LockError, StateDirectoryLock};
    use rustix::fd::OwnedFd;
    use rustix::fs::{fcntl_getfl, fcntl_setfl, OFlags};
    use rustix::io::{fcntl_getfd, fcntl_setfd, Errno, FdFlags};
    use rustix::process::{kill_process_group, Pid, Signal};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::path::{Path, PathBuf};
    use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    const HARNESS_MODE: &str = "CHECKSY_PROCESS_HARNESS_MODE";
    const HARNESS_ROOT: &str = "CHECKSY_PROCESS_HARNESS_ROOT";
    const HARNESS_NONCE: &str = "CHECKSY_PROCESS_HARNESS_NONCE";
    const HARNESS_PGID: &str = "CHECKSY_PROCESS_HARNESS_PGID";
    const HELPER_TEST: &str = "process_runner::tests::process_harness_helper";
    const INNER_WATCHDOG: Duration = Duration::from_secs(8);
    const OUTER_WATCHDOG: Duration = Duration::from_secs(15);
    const READINESS_TIMEOUT: Duration = Duration::from_secs(6);
    static NEXT_NONCE: AtomicU64 = AtomicU64::new(1);

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

    fn harness_command(mode: &str, root: &Path, nonce: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--ignored")
            .arg("--exact")
            .arg(HELPER_TEST)
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env(HARNESS_MODE, mode)
            .env(HARNESS_ROOT, root)
            .env(HARNESS_NONCE, nonce);
        command
    }

    fn term_ignoring_harness_command(mode: &str, root: &Path, nonce: &str) -> Command {
        let helper = harness_command(mode, root, nonce);
        let program = helper.get_program().to_owned();
        let arguments: Vec<_> = helper.get_args().map(ToOwned::to_owned).collect();
        let environment: Vec<_> = helper
            .get_envs()
            .map(|(key, value)| (key.to_owned(), value.map(ToOwned::to_owned)))
            .collect();
        let mut command = Command::new("bash");
        command
            .arg("-c")
            .arg("trap '' TERM; exec \"$@\"")
            .arg("checksy-term-wrapper")
            .arg(program)
            .args(arguments);
        for (key, value) in environment {
            match value {
                Some(value) => {
                    command.env(key, value);
                }
                None => {
                    command.env_remove(key);
                }
            }
        }
        command
    }

    fn next_nonce() -> String {
        format!(
            "{}-{}",
            std::process::id(),
            NEXT_NONCE.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn pid_as_u32(pid: Pid) -> u32 {
        #[cfg(target_os = "linux")]
        {
            Pid::as_raw(Some(pid))
        }
        #[cfg(target_os = "macos")]
        {
            u32::try_from(Pid::as_raw(Some(pid))).expect("test process ID must fit in u32")
        }
    }

    fn pid_from_u32(raw: u32) -> Pid {
        #[cfg(target_os = "linux")]
        let raw_pid = raw;
        #[cfg(target_os = "macos")]
        let raw_pid = rustix::process::RawPid::try_from(raw)
            .expect("reported test process ID must fit the target pid type");
        // SAFETY: all callers use a nonzero process ID reported by Child or
        // the runner's spawn observer.
        unsafe { Pid::from_raw(raw_pid) }.expect("test process ID cannot be zero")
    }

    fn liveness_pipe() -> (OwnedFd, OwnedFd) {
        let (reader, writer) = rustix::io::pipe().expect("create harness liveness pipe");
        let reader_flags = fcntl_getfd(&reader).expect("inspect liveness reader flags");
        fcntl_setfd(&reader, reader_flags | FdFlags::CLOEXEC)
            .expect("make liveness reader close on exec");
        let writer_flags = fcntl_getfd(&writer).expect("inspect liveness writer flags");
        fcntl_setfd(&writer, writer_flags - FdFlags::CLOEXEC)
            .expect("make liveness writer survive exec");
        let status = fcntl_getfl(&reader).expect("inspect liveness reader status");
        fcntl_setfl(&reader, status | OFlags::NONBLOCK).expect("make liveness reader nonblocking");
        (reader, writer)
    }

    fn liveness_writer_is_open(reader: &OwnedFd) -> bool {
        let mut byte = [0_u8; 1];
        loop {
            match rustix::io::read(reader, &mut byte) {
                Ok(0) => return false,
                Ok(_) | Err(Errno::AGAIN) => return true,
                Err(Errno::INTR) => continue,
                Err(error) => panic!("inspect harness liveness pipe: {error}"),
            }
        }
    }

    fn pause_until(deadline: Instant) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        let millis = i32::try_from(remaining.as_millis().clamp(1, 25)).unwrap_or(25);
        let _ = rustix::io::poll(&mut [], millis);
    }

    fn try_wait_until(child: &mut Child, deadline: Instant) -> std::io::Result<Option<ExitStatus>> {
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(Some(status));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            pause_until(deadline);
        }
    }

    struct ChildCleanup {
        child: Option<Child>,
        process_group: Option<Pid>,
        inner_process_group: Option<Arc<Mutex<Option<Pid>>>>,
        inner_liveness: Option<PathBuf>,
    }

    impl ChildCleanup {
        fn new(child: Child, process_group: Option<Pid>) -> Self {
            Self {
                child: Some(child),
                process_group,
                inner_process_group: None,
                inner_liveness: None,
            }
        }

        fn track_inner_group(&mut self, process_group: Arc<Mutex<Option<Pid>>>, liveness: PathBuf) {
            self.inner_process_group = Some(process_group);
            self.inner_liveness = Some(liveness);
        }

        fn child_mut(&mut self) -> &mut Child {
            self.child.as_mut().expect("child guard is armed")
        }

        fn disarm(&mut self) {
            self.child.take();
        }

        fn cleanup(&mut self) {
            // Take every identity-bearing target before signaling. Explicit
            // cleanup followed by Drop must not signal a recycled PGID.
            let Some(mut child) = self.child.take() else {
                return;
            };
            let inner_process_group = self.inner_process_group.take();
            let inner_liveness = self.inner_liveness.take();
            let process_group = self.process_group.take();
            if let (Some(group), Some(liveness)) = (inner_process_group, inner_liveness.as_deref())
            {
                let live_group = *group.lock().unwrap();
                if let Some(group) = live_group {
                    if matches!(StateDirectoryLock::acquire(liveness), Err(LockError::Held)) {
                        let _ = kill_process_group(group, Signal::Kill);
                    }
                }
            }
            if let Some(group) = process_group {
                let _ = kill_process_group(group, Signal::Kill);
            }
            let _ = child.kill();
            let deadline = Instant::now() + Duration::from_secs(1);
            let _ = try_wait_until(&mut child, deadline);
        }
    }

    impl Drop for ChildCleanup {
        fn drop(&mut self) {
            self.cleanup();
        }
    }

    fn read_pipe(
        pipe: impl Read + Send + 'static,
        group_report: Option<(String, Arc<Mutex<Option<Pid>>>)>,
    ) -> mpsc::Receiver<Vec<u8>> {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut reader = BufReader::new(pipe);
            loop {
                let mut line = Vec::new();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        if let Some((marker, group)) = &group_report {
                            let text = String::from_utf8_lossy(&line);
                            if let Some(raw) = text
                                .split(marker)
                                .nth(1)
                                .and_then(|value| value.trim().parse::<u32>().ok())
                            {
                                // The runner observer reports this nonzero
                                // child process group directly.
                                *group.lock().unwrap() = Some(pid_from_u32(raw));
                            }
                        }
                        bytes.extend_from_slice(&line);
                    }
                    Err(_) => break,
                }
            }
            let _ = sender.send(bytes);
        });
        receiver
    }

    fn run_isolated_scenario(mode: &str) {
        let temp = tempdir().unwrap();
        let nonce = next_nonce();
        let mut command = harness_command(mode, temp.path(), &nonce);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);

        let child = command.spawn().unwrap();
        let helper_group = Pid::from_child(&child);
        let mut child = ChildCleanup::new(child, Some(helper_group));
        let inner_group = Arc::new(Mutex::new(None));
        child.track_inner_group(inner_group.clone(), lock_path(temp.path(), "leader"));
        let stdout = read_pipe(
            child.child_mut().stdout.take().unwrap(),
            Some((format!("RUNNER_PGID:{nonce}:"), inner_group)),
        );
        let stderr = read_pipe(child.child_mut().stderr.take().unwrap(), None);
        let deadline = Instant::now() + OUTER_WATCHDOG;
        let status = match try_wait_until(child.child_mut(), deadline).unwrap() {
            Some(status) => status,
            None => {
                child.cleanup();
                let stdout = stdout
                    .recv_timeout(Duration::from_secs(1))
                    .unwrap_or_default();
                let stderr = stderr
                    .recv_timeout(Duration::from_secs(1))
                    .unwrap_or_default();
                panic!(
                    "isolated process scenario exceeded outer watchdog\nstdout:\n{}\nstderr:\n{}",
                    String::from_utf8_lossy(&stdout),
                    String::from_utf8_lossy(&stderr)
                );
            }
        };
        child.disarm();
        let stdout = stdout.recv_timeout(Duration::from_secs(1)).unwrap();
        let stderr = stderr.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(
            status.success(),
            "isolated process scenario failed with {status}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
    }

    fn wait_for_nonce_line(
        stdout: ChildStdout,
        marker: String,
    ) -> Result<String, mpsc::RecvTimeoutError> {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => return,
                    Ok(_) if line.contains(&marker) => {
                        let _ = sender.send(line);
                        return;
                    }
                    Ok(_) => {}
                    Err(_) => return,
                }
            }
        });
        receiver.recv_timeout(READINESS_TIMEOUT)
    }

    fn park_forever() -> ! {
        loop {
            thread::park();
        }
    }

    fn lock_path(root: &Path, role: &str) -> PathBuf {
        root.join(format!("{role}-lock"))
    }

    fn reacquire_after_death(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            match StateDirectoryLock::acquire(path) {
                Ok(lock) => {
                    drop(lock);
                    return;
                }
                Err(LockError::Held) if Instant::now() < deadline => pause_until(deadline),
                Err(error) => panic!(
                    "death sentinel '{}' was not released: {error}",
                    path.display()
                ),
            }
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum WatchdogMessage {
        Spawned(u32),
        LeaderReaped,
        Done,
    }

    fn inner_watchdog(
        receiver: mpsc::Receiver<WatchdogMessage>,
        liveness_reader: OwnedFd,
    ) -> thread::JoinHandle<bool> {
        thread::spawn(move || {
            let deadline = Instant::now() + INNER_WATCHDOG;
            let mut process_group = None;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                match receiver.recv_timeout(remaining) {
                    Ok(WatchdogMessage::Spawned(group)) => process_group = Some(group),
                    Ok(WatchdogMessage::LeaderReaped | WatchdogMessage::Done) => return false,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return false,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        let Some(raw_group) = process_group else {
                            return false;
                        };
                        // The parent writer was dropped by the Spawned
                        // callback. A still-open inherited writer proves that
                        // this exact runner child tree remains alive, including
                        // before its Rust helper can acquire a sentinel lock.
                        if !liveness_writer_is_open(&liveness_reader) {
                            return false;
                        }
                        let group = pid_from_u32(raw_group);
                        let _ = kill_process_group(group, Signal::Kill);
                        return true;
                    }
                }
            }
        })
    }

    fn finish_watchdog(handle: thread::JoinHandle<bool>) -> bool {
        let deadline = Instant::now() + Duration::from_secs(1);
        while !handle.is_finished() && Instant::now() < deadline {
            pause_until(deadline);
        }
        assert!(
            handle.is_finished(),
            "inner watchdog failed to acknowledge completion"
        );
        handle.join().expect("inner watchdog panicked")
    }

    fn run_tree_scenario(root: &Path, nonce: &str) {
        let leader_lock = lock_path(root, "leader");
        let child_lock = lock_path(root, "child");
        let grandchild_lock = lock_path(root, "grandchild");
        let (watchdog_sender, watchdog_receiver) = mpsc::channel();
        let (liveness_reader, liveness_writer) = liveness_pipe();
        let mut liveness_writer = Some(liveness_writer);
        let watchdog = inner_watchdog(watchdog_receiver, liveness_reader);
        let mut events = Vec::new();
        let command = term_ignoring_harness_command("leader", root, nonce);
        let limits = ProcessLimits {
            timeout: Duration::from_secs(3),
            term_grace: Duration::from_millis(100),
        };
        let result = run_observed(command, limits, |event| {
            events.push(event);
            let message = match event {
                ProcessTestEvent::Spawned { process_group } => {
                    drop(liveness_writer.take());
                    println!("RUNNER_PGID:{nonce}:{process_group}");
                    std::io::stdout().flush().unwrap();
                    Some(WatchdogMessage::Spawned(process_group))
                }
                ProcessTestEvent::LeaderReaped => Some(WatchdogMessage::LeaderReaped),
                ProcessTestEvent::TermSent | ProcessTestEvent::KillSent => None,
            };
            if let Some(message) = message {
                let _ = watchdog_sender.send(message);
            }
        });
        drop(liveness_writer.take());
        let _ = watchdog_sender.send(WatchdogMessage::Done);
        assert!(!finish_watchdog(watchdog), "inner watchdog fired");

        let (output, timeout) = match result {
            Err(ProcessError::TimedOut { timeout, output }) => (output, timeout),
            other => panic!("expected managed-tree timeout, got {other:?}"),
        };
        assert_eq!(timeout, limits.timeout);
        assert_eq!(
            output.status.and_then(|status| status.signal()),
            Some(Signal::Kill as i32)
        );
        let stdout = output.stdout.render_lossy();
        let stderr = output.stderr.render_lossy();
        assert!(
            stdout.contains(&format!("PRE_TIMEOUT_STDOUT:{nonce}")),
            "{stdout}"
        );
        assert!(
            stderr.contains(&format!("PRE_TIMEOUT_STDERR:{nonce}")),
            "{stderr}"
        );

        let tree_marker = format!("TREE_READY:{nonce}:");
        let tree_line = stdout
            .lines()
            .find_map(|line| {
                line.find(&tree_marker)
                    .map(|at| &line[at + tree_marker.len()..])
            })
            .expect("leader did not report the complete ready tree");
        let pids: Vec<u32> = tree_line
            .split(':')
            .take(3)
            .map(|value| value.trim().parse().unwrap())
            .collect();
        assert_eq!(pids.len(), 3);
        assert_eq!(
            events.first(),
            Some(&ProcessTestEvent::Spawned {
                process_group: pids[0]
            })
        );
        assert_eq!(
            events,
            vec![
                ProcessTestEvent::Spawned {
                    process_group: pids[0],
                },
                ProcessTestEvent::TermSent,
                ProcessTestEvent::KillSent,
                ProcessTestEvent::LeaderReaped,
            ]
        );

        reacquire_after_death(&leader_lock);
        reacquire_after_death(&child_lock);
        reacquire_after_death(&grandchild_lock);
    }

    fn run_nonzero_scenario(_root: &Path, nonce: &str) {
        let (watchdog_sender, watchdog_receiver) = mpsc::channel();
        let (liveness_reader, liveness_writer) = liveness_pipe();
        let mut liveness_writer = Some(liveness_writer);
        let watchdog = inner_watchdog(watchdog_receiver, liveness_reader);
        let mut events = Vec::new();
        let mut command = Command::new("bash");
        command
            .arg("-c")
            .arg(
                "printf 'NONZERO_STDOUT:%s' \"$1\"; printf 'NONZERO_STDERR:%s' \"$1\" >&2; exit 23",
            )
            .arg("checksy-nonzero")
            .arg(nonce);
        let completed = run_observed(command, short_limits(), |event| {
            events.push(event);
            let message = match event {
                ProcessTestEvent::Spawned { process_group } => {
                    drop(liveness_writer.take());
                    println!("RUNNER_PGID:{nonce}:{process_group}");
                    std::io::stdout().flush().unwrap();
                    Some(WatchdogMessage::Spawned(process_group))
                }
                ProcessTestEvent::LeaderReaped => Some(WatchdogMessage::LeaderReaped),
                ProcessTestEvent::TermSent | ProcessTestEvent::KillSent => None,
            };
            if let Some(message) = message {
                let _ = watchdog_sender.send(message);
            }
        })
        .expect("ordinary nonzero exit must be a completed process");
        drop(liveness_writer.take());
        let _ = watchdog_sender.send(WatchdogMessage::Done);
        assert!(!finish_watchdog(watchdog), "inner watchdog fired");
        assert_eq!(completed.status.code(), Some(23));
        assert_eq!(
            completed.stdout.bytes,
            format!("NONZERO_STDOUT:{nonce}").as_bytes()
        );
        assert_eq!(
            completed.stderr.bytes,
            format!("NONZERO_STDERR:{nonce}").as_bytes()
        );
        assert!(matches!(
            events.as_slice(),
            [
                ProcessTestEvent::Spawned { .. },
                ProcessTestEvent::LeaderReaped
            ]
        ));
    }

    #[test]
    fn managed_tree_forced_timeout() {
        run_isolated_scenario("scenario-tree");
    }

    #[test]
    fn pre_timeout_output_retained() {
        // Use the same fully managed tree so retained output is proven only
        // after both descendant liveness sentinels were established.
        run_isolated_scenario("scenario-tree");
    }

    #[test]
    fn ordinary_nonzero_distinct() {
        run_isolated_scenario("scenario-nonzero");
    }

    #[test]
    #[ignore = "subprocess helper invoked by deterministic process tests"]
    fn process_harness_helper() {
        let Ok(mode) = std::env::var(HARNESS_MODE) else {
            // `cargo test -- --ignored` should not park accidentally.
            return;
        };
        let root = PathBuf::from(std::env::var_os(HARNESS_ROOT).expect("harness root"));
        let nonce = std::env::var(HARNESS_NONCE).expect("harness nonce");

        match mode.as_str() {
            "scenario-tree" => run_tree_scenario(&root, &nonce),
            "scenario-nonzero" => run_nonzero_scenario(&root, &nonce),
            "leader" => {
                let _leader_lock =
                    StateDirectoryLock::acquire(&lock_path(&root, "leader")).unwrap();
                let leader_pid = std::process::id();
                assert_eq!(pid_as_u32(rustix::process::getpgrp()), leader_pid);
                let mut command = harness_command("child", &root, &nonce);
                command
                    .env(HARNESS_PGID, leader_pid.to_string())
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());
                let child = command.spawn().unwrap();
                let child_pid = child.id();
                let mut child = ChildCleanup::new(child, None);
                let line = wait_for_nonce_line(
                    child.child_mut().stdout.take().unwrap(),
                    format!("CHILD_READY:{nonce}:"),
                )
                .expect("child readiness timed out");
                let suffix = line.split(&format!("CHILD_READY:{nonce}:")).nth(1).unwrap();
                let reported: Vec<u32> = suffix
                    .split(':')
                    .take(2)
                    .map(|value| value.trim().parse().unwrap())
                    .collect();
                assert_eq!(reported[0], child_pid);
                println!(
                    "TREE_READY:{nonce}:{leader_pid}:{}:{}",
                    reported[0], reported[1]
                );
                println!("PRE_TIMEOUT_STDOUT:{nonce}");
                eprintln!("PRE_TIMEOUT_STDERR:{nonce}");
                std::io::stdout().flush().unwrap();
                std::io::stderr().flush().unwrap();
                park_forever();
            }
            "child" => {
                let expected_group: u32 = std::env::var(HARNESS_PGID).unwrap().parse().unwrap();
                assert_eq!(pid_as_u32(rustix::process::getpgrp()), expected_group);
                let _child_lock = StateDirectoryLock::acquire(&lock_path(&root, "child")).unwrap();
                let child_pid = std::process::id();
                let mut command = harness_command("grandchild", &root, &nonce);
                command
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());
                let grandchild = command.spawn().unwrap();
                let grandchild_pid = grandchild.id();
                let mut grandchild = ChildCleanup::new(grandchild, None);
                wait_for_nonce_line(
                    grandchild.child_mut().stdout.take().unwrap(),
                    format!("GRANDCHILD_READY:{nonce}:{grandchild_pid}"),
                )
                .expect("grandchild readiness timed out");
                println!("CHILD_READY:{nonce}:{child_pid}:{grandchild_pid}");
                std::io::stdout().flush().unwrap();
                park_forever();
            }
            "grandchild" => {
                let expected_group: u32 = std::env::var(HARNESS_PGID).unwrap().parse().unwrap();
                assert_eq!(pid_as_u32(rustix::process::getpgrp()), expected_group);
                let _grandchild_lock =
                    StateDirectoryLock::acquire(&lock_path(&root, "grandchild")).unwrap();
                println!("GRANDCHILD_READY:{nonce}:{}", std::process::id());
                std::io::stdout().flush().unwrap();
                park_forever();
            }
            other => panic!("unknown process harness mode: {other}"),
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
    fn continuous_output_drain() {
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
    fn capture_exact_limit() {
        let script = format!(
            "produce() {{ printf H; head -c {} /dev/zero; printf T; }}; produce; produce >&2",
            CAPTURE_LIMIT_BYTES - 2
        );
        let completed = run(command(&script), short_limits()).unwrap();

        for captured in [&completed.stdout, &completed.stderr] {
            assert_eq!(captured.bytes.len(), CAPTURE_LIMIT_BYTES);
            assert_eq!(captured.original_bytes, CAPTURE_LIMIT_BYTES as u64);
            assert!(!captured.truncated);
            assert_eq!(captured.bytes.first(), Some(&b'H'));
            assert_eq!(captured.bytes.last(), Some(&b'T'));
        }
    }

    #[test]
    fn capture_max_plus_one() {
        let script = format!(
            "produce() {{ printf H; head -c {} /dev/zero; printf T; }}; produce; produce >&2",
            CAPTURE_LIMIT_BYTES - 1
        );
        let completed = run(command(&script), short_limits()).unwrap();

        let expected_head = [vec![b'H'], vec![0; CAPTURE_LIMIT_BYTES / 2 - 1]].concat();
        let expected_tail = [vec![0; CAPTURE_LIMIT_BYTES / 2 - 1], vec![b'T']].concat();
        for captured in [&completed.stdout, &completed.stderr] {
            assert_eq!(captured.bytes.len(), CAPTURE_LIMIT_BYTES);
            assert_eq!(captured.original_bytes, (CAPTURE_LIMIT_BYTES + 1) as u64);
            assert!(captured.truncated);
            assert_eq!(captured.bytes.first(), Some(&b'H'));
            assert_eq!(captured.bytes.last(), Some(&b'T'));
            assert_eq!(
                &captured.bytes[..CAPTURE_LIMIT_BYTES / 2],
                expected_head.as_slice()
            );
            assert_eq!(
                &captured.bytes[CAPTURE_LIMIT_BYTES / 2..],
                expected_tail.as_slice()
            );
        }
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
