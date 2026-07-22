use std::fmt;
use std::io;
use std::process::{Command, ExitStatus};
use std::time::Duration;

pub(crate) const CAPTURE_LIMIT_BYTES: usize = 1024 * 1024;
pub(crate) const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(15 * 60);
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
    /// The complete stream when untruncated, otherwise equal head and tail
    /// halves of the bounded stream.
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

pub(crate) struct SignalRestoreGuard {
    restore: Option<Box<dyn FnOnce() -> io::Result<()>>>,
}

impl SignalRestoreGuard {
    fn new(restore: impl FnOnce() -> io::Result<()> + 'static) -> Self {
        Self {
            restore: Some(Box::new(restore)),
        }
    }

    fn disarmed() -> Self {
        Self { restore: None }
    }

    fn restore(&mut self) -> io::Result<()> {
        match self.restore.take() {
            Some(restore) => restore(),
            None => Ok(()),
        }
    }
}

impl fmt::Debug for SignalRestoreGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignalRestoreGuard")
            .field("armed", &self.restore.is_some())
            .finish()
    }
}

impl Drop for SignalRestoreGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
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
    ChildSignaled {
        signal: i32,
        output: PartialProcessOutput,
    },
    Interrupted {
        signal: i32,
        output: PartialProcessOutput,
        restore: SignalRestoreGuard,
    },
    #[allow(dead_code)] // Constructed by `run` on unsupported target builds.
    UnsupportedPlatform,
}

impl ProcessError {
    pub(crate) fn output(&self) -> Option<&PartialProcessOutput> {
        match self {
            Self::Supervision { output, .. }
            | Self::TimedOut { output, .. }
            | Self::ChildSignaled { output, .. }
            | Self::Interrupted { output, .. } => Some(output),
            Self::Spawn(_) | Self::UnsupportedPlatform => None,
        }
    }

    pub(crate) fn restore_signal_handlers(&mut self) -> io::Result<()> {
        match self {
            Self::Interrupted { restore, .. } => restore.restore(),
            _ => Ok(()),
        }
    }
}

impl fmt::Display for ProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "failed to spawn process: {error}"),
            Self::Supervision { source, output } => {
                write!(formatter, "failed to supervise process: {source}")?;
                write_status(formatter, output.status)
            }
            Self::TimedOut { timeout, output } => {
                write!(
                    formatter,
                    "process timed out after {}ms",
                    timeout.as_millis()
                )?;
                write_status(formatter, output.status)
            }
            Self::ChildSignaled { signal, output } => {
                write!(formatter, "process was terminated by signal {signal}")?;
                write_status(formatter, output.status)
            }
            Self::Interrupted { signal, output, .. } => {
                write!(formatter, "process interrupted by parent signal {signal}")?;
                write_status(formatter, output.status)
            }
            Self::UnsupportedPlatform => {
                formatter.write_str("process supervision is supported only on Linux and macOS")
            }
        }
    }
}

fn write_status(formatter: &mut fmt::Formatter<'_>, status: Option<ExitStatus>) -> fmt::Result {
    if let Some(status) = status {
        write!(formatter, " (leader status: {status})")?;
    }
    Ok(())
}

impl std::error::Error for ProcessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn(error) | Self::Supervision { source: error, .. } => Some(error),
            Self::TimedOut { .. }
            | Self::ChildSignaled { .. }
            | Self::Interrupted { .. }
            | Self::UnsupportedPlatform => None,
        }
    }
}

/// Run a configured command under Checksy's bounded process supervisor.
///
/// The caller owns arguments, environment, and working directory. This
/// function overrides all three standard streams: stdin is `/dev/null`, while
/// stdout and stderr are independently drained pipes.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn run(
    command: Command,
    limits: ProcessLimits,
) -> Result<CompletedProcess, ProcessError> {
    supported::run(command, limits)
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(crate) fn run_observed(
    command: Command,
    limits: ProcessLimits,
    observer: impl FnMut(ProcessTestEvent),
) -> Result<CompletedProcess, ProcessError> {
    supported::run_observed(command, limits, observer)
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProcessTestEvent {
    Spawned { process_group: u32 },
    ParentSignalForwarded { signal: i32 },
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
    use rustix::process::{
        kill_process_group, test_kill_process_group, waitid, Pid, Signal, WaitId, WaitidOptions,
    };
    use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
    use std::collections::VecDeque;
    use std::io;
    use std::mem;
    use std::os::unix::process::CommandExt;
    use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
    use std::ptr;
    use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
    use std::sync::{Mutex, MutexGuard};
    use std::time::{Duration, Instant};

    const READ_BUFFER_BYTES: usize = 32 * 1024;
    const MAX_DRAIN_BYTES_PER_PASS: usize = 256 * 1024;
    const SUPERVISION_TICK: Duration = Duration::from_millis(25);
    const FINAL_REAP_LIMIT: Duration = Duration::from_secs(5);

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
        fn parent_signal_forwarded(&mut self, _signal: i32) {}
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

        fn parent_signal_forwarded(&mut self, signal: i32) {
            (self.0)(ProcessTestEvent::ParentSignalForwarded { signal });
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

    #[derive(Clone, Copy, Debug)]
    enum LeaderObservation {
        Exited,
        Signaled(i32),
    }

    #[derive(Clone, Copy, Debug)]
    enum TerminationCause {
        Timeout,
        ChildSignal(i32),
    }

    fn run_impl<O: ProcessObserver>(
        command: Command,
        limits: ProcessLimits,
        observer: &mut O,
    ) -> Result<CompletedProcess, ProcessError> {
        validate_limits(limits)?;
        let signal_handlers =
            TemporarySignalHandlers::install().map_err(|source| ProcessError::Supervision {
                source,
                output: PartialProcessOutput::empty(),
            })?;
        let result = run_with_signal_handlers(command, limits, observer, &signal_handlers);
        finalize_signal_handlers(result, signal_handlers)
    }

    fn run_with_signal_handlers<O: ProcessObserver>(
        mut command: Command,
        limits: ProcessLimits,
        observer: &mut O,
        signal_handlers: &TemporarySignalHandlers,
    ) -> Result<CompletedProcess, ProcessError> {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // SAFETY: `setsid` is async-signal-safe and this closure performs no
        // allocation or other work between fork and exec.
        unsafe {
            command.pre_exec(|| rustix::process::setsid().map(|_| ()).map_err(os_error));
        }

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

        set_nonblocking(&stdout).map_err(|source| ProcessError::Supervision {
            source,
            output: PartialProcessOutput::empty(),
        })?;
        set_nonblocking(&stderr).map_err(|source| ProcessError::Supervision {
            source,
            output: PartialProcessOutput::empty(),
        })?;

        let started_at = Instant::now();
        let timeout_at = started_at.checked_add(limits.timeout).unwrap_or(started_at);
        let mut stdout_capture = CaptureBuffer::new(CAPTURE_LIMIT_BYTES);
        let mut stderr_capture = CaptureBuffer::new(CAPTURE_LIMIT_BYTES);
        let mut stdout_open = true;
        let mut stderr_open = true;
        let mut observation = None;
        let mut cause = None;
        let mut first_parent_signal = None;
        let mut term_deadline = None;
        let mut kill_deadline = None;
        let mut kill_sent = false;

        loop {
            let now = Instant::now();

            let received_signals = signal_handlers.take_count();
            for _ in 0..received_signals {
                if first_parent_signal.is_none() {
                    let signal = signal_handlers.first_signal().ok_or_else(|| {
                        supervision_with_output(
                            io::Error::other("termination signal counter lost its first signal"),
                            &child,
                            &stdout_capture,
                            &stderr_capture,
                        )
                    })?;
                    first_parent_signal = Some(signal);
                    let forwarded = Signal::from_raw(signal).ok_or_else(|| {
                        supervision_with_output(
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("unsupported caught signal {signal}"),
                            ),
                            &child,
                            &stdout_capture,
                            &stderr_capture,
                        )
                    })?;
                    signal_group(child.process_group(), forwarded).map_err(|source| {
                        supervision_with_output(source, &child, &stdout_capture, &stderr_capture)
                    })?;
                    observer.parent_signal_forwarded(signal);
                    if forwarded == Signal::Term {
                        observer.term_sent();
                    }
                    if term_deadline.is_none() {
                        term_deadline = Some(deadline_from(now, limits.term_grace));
                    }
                } else if !kill_sent {
                    send_kill(
                        &mut child,
                        observer,
                        &mut kill_sent,
                        &mut kill_deadline,
                        &stdout_capture,
                        &stderr_capture,
                    )?;
                }
            }

            if observation.is_none() && child.status().is_none() {
                observation = peek_leader(child.process_group()).map_err(|source| {
                    supervision_with_output(source, &child, &stdout_capture, &stderr_capture)
                })?;
                if let Some(LeaderObservation::Signaled(signal)) = observation {
                    if cause.is_none() && first_parent_signal.is_none() {
                        cause = Some(TerminationCause::ChildSignal(signal));
                        signal_group(child.process_group(), Signal::Term).map_err(|source| {
                            supervision_with_output(
                                source,
                                &child,
                                &stdout_capture,
                                &stderr_capture,
                            )
                        })?;
                        observer.term_sent();
                        term_deadline = Some(deadline_from(now, limits.term_grace));
                    }
                }
            }

            if cause.is_none() && first_parent_signal.is_none() && now >= timeout_at {
                cause = Some(TerminationCause::Timeout);
                signal_group(child.process_group(), Signal::Term).map_err(|source| {
                    supervision_with_output(source, &child, &stdout_capture, &stderr_capture)
                })?;
                observer.term_sent();
                term_deadline = Some(deadline_from(now, limits.term_grace));
            }

            let terminating = cause.is_some() || first_parent_signal.is_some();
            if terminating && !kill_sent {
                if let Some(deadline) = term_deadline {
                    if now >= deadline {
                        send_kill(
                            &mut child,
                            observer,
                            &mut kill_sent,
                            &mut kill_deadline,
                            &stdout_capture,
                            &stderr_capture,
                        )?;
                    }
                }
            }

            if kill_sent && child.status().is_none() {
                reap_if_ready(&mut child, observer).map_err(|source| {
                    supervision_with_output(source, &child, &stdout_capture, &stderr_capture)
                })?;
            }

            if !terminating && !stdout_open && !stderr_open {
                if observation.is_some() {
                    reap_ready_leader(&mut child, observer).map_err(|source| {
                        supervision_with_output(source, &child, &stdout_capture, &stderr_capture)
                    })?;
                } else {
                    reap_if_ready(&mut child, observer).map_err(|source| {
                        supervision_with_output(source, &child, &stdout_capture, &stderr_capture)
                    })?;
                }
                if let Some(status) = child.status() {
                    let group_exists =
                        process_group_exists(child.process_group()).map_err(|source| {
                            supervision_with_output(
                                source,
                                &child,
                                &stdout_capture,
                                &stderr_capture,
                            )
                        })?;
                    if !group_exists {
                        child.disarm();
                        return Ok(CompletedProcess {
                            status,
                            stdout: stdout_capture.finish(),
                            stderr: stderr_capture.finish(),
                        });
                    }
                }
            }

            if terminating && !stdout_open && !stderr_open {
                if child.status().is_none() && observation.is_some() {
                    reap_ready_leader(&mut child, observer).map_err(|source| {
                        supervision_with_output(source, &child, &stdout_capture, &stderr_capture)
                    })?;
                }
                if child.status().is_some()
                    && !process_group_exists(child.process_group()).map_err(|source| {
                        supervision_with_output(source, &child, &stdout_capture, &stderr_capture)
                    })?
                {
                    child.disarm();
                    return Err(termination_error(
                        first_parent_signal,
                        cause,
                        limits.timeout,
                        &child,
                        &stdout_capture,
                        &stderr_capture,
                    ));
                }
            }

            if let Some(deadline) = kill_deadline {
                if now >= deadline {
                    if child.status().is_none() {
                        reap_if_ready(&mut child, observer).map_err(|source| {
                            supervision_with_output(
                                source,
                                &child,
                                &stdout_capture,
                                &stderr_capture,
                            )
                        })?;
                    }
                    if child.status().is_some() {
                        child.disarm();
                    }
                    return Err(termination_error(
                        first_parent_signal,
                        cause,
                        limits.timeout,
                        &child,
                        &stdout_capture,
                        &stderr_capture,
                    ));
                }
            }

            let wake_at = next_wake(timeout_at, term_deadline, kill_deadline);
            poll_streams(
                stdout_open.then_some(&stdout),
                stderr_open.then_some(&stderr),
                wake_at,
            )
            .map_err(|source| {
                supervision_with_output(source, &child, &stdout_capture, &stderr_capture)
            })?;

            if stdout_open {
                stdout_open = drain_stream(&mut stdout, &mut stdout_capture).map_err(|source| {
                    supervision_with_output(source, &child, &stdout_capture, &stderr_capture)
                })?;
            }
            if stderr_open {
                stderr_open = drain_stream(&mut stderr, &mut stderr_capture).map_err(|source| {
                    supervision_with_output(source, &child, &stdout_capture, &stderr_capture)
                })?;
            }
        }
    }

    fn finalize_signal_handlers(
        result: Result<CompletedProcess, ProcessError>,
        mut signal_handlers: TemporarySignalHandlers,
    ) -> Result<CompletedProcess, ProcessError> {
        let result = match result {
            Err(ProcessError::Interrupted { signal, output, .. }) => {
                let restore = super::SignalRestoreGuard::new(move || signal_handlers.restore());
                return Err(ProcessError::Interrupted {
                    signal,
                    output,
                    restore,
                });
            }
            result => result,
        };

        let restore_result = signal_handlers.restore();
        let late_signals = signal_handlers.take_count();
        if late_signals > 0 {
            let output = result_partial_output(&result);
            let Some(signal) = signal_handlers.first_signal() else {
                return Err(ProcessError::Supervision {
                    source: io::Error::other(
                        "termination signal counter lost its first signal during cleanup",
                    ),
                    output,
                });
            };
            return Err(ProcessError::Interrupted {
                signal,
                output,
                restore: super::SignalRestoreGuard::disarmed(),
            });
        }

        if let Err(source) = restore_result {
            return Err(ProcessError::Supervision {
                source,
                output: result_partial_output(&result),
            });
        }
        result
    }

    fn result_partial_output(
        result: &Result<CompletedProcess, ProcessError>,
    ) -> PartialProcessOutput {
        match result {
            Ok(completed) => PartialProcessOutput {
                status: Some(completed.status),
                stdout: completed.stdout.clone(),
                stderr: completed.stderr.clone(),
            },
            Err(error) => error
                .output()
                .cloned()
                .unwrap_or_else(PartialProcessOutput::empty),
        }
    }

    fn validate_limits(limits: ProcessLimits) -> Result<(), ProcessError> {
        let message = if limits.timeout.is_zero() {
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

        match message {
            Some(message) => Err(ProcessError::Supervision {
                source: io::Error::new(io::ErrorKind::InvalidInput, message),
                output: PartialProcessOutput::empty(),
            }),
            None => Ok(()),
        }
    }

    static SIGNAL_HANDLER_LOCK: Mutex<()> = Mutex::new(());
    static SIGNAL_COUNT: AtomicUsize = AtomicUsize::new(0);
    static FIRST_SIGNAL: AtomicI32 = AtomicI32::new(0);

    extern "C" fn capture_termination_signal(signal: libc::c_int) {
        let _ = FIRST_SIGNAL.compare_exchange(0, signal, Ordering::Release, Ordering::Relaxed);
        SIGNAL_COUNT.fetch_add(1, Ordering::Release);
    }

    struct PreviousSignalAction {
        signal: libc::c_int,
        action: libc::sigaction,
    }

    /// Owns exact process signal dispositions for one supervised command.
    ///
    /// Signal dispositions are process-global, so runner calls are serialized
    /// while these temporary handlers are installed. The original actions are
    /// restored before normal/operational returns and are retained through CLI
    /// diagnostic flushing for a caught parent interruption.
    pub(super) struct TemporarySignalHandlers {
        previous: Vec<PreviousSignalAction>,
        _serial: MutexGuard<'static, ()>,
    }

    impl TemporarySignalHandlers {
        fn install() -> Result<Self, io::Error> {
            let serial = SIGNAL_HANDLER_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            SIGNAL_COUNT.store(0, Ordering::Release);
            FIRST_SIGNAL.store(0, Ordering::Release);

            let mut handlers = Self {
                previous: Vec::with_capacity(4),
                _serial: serial,
            };
            for signal in [SIGINT, SIGTERM, SIGHUP, SIGQUIT] {
                // SAFETY: sigaction structures must be initialized before use.
                // The handler performs lock-free atomic operations only.
                let mut action: libc::sigaction = unsafe { mem::zeroed() };
                action.sa_sigaction = capture_termination_signal as *const () as usize;
                action.sa_flags = libc::SA_RESTART;
                // SAFETY: `sa_mask` is a valid field of the initialized action.
                if unsafe { libc::sigemptyset(&mut action.sa_mask) } != 0 {
                    return Err(io::Error::last_os_error());
                }

                // SAFETY: all pointers refer to initialized sigaction storage
                // that remains valid for the duration of this call.
                let mut previous: libc::sigaction = unsafe { mem::zeroed() };
                if unsafe { libc::sigaction(signal, &action, &mut previous) } != 0 {
                    return Err(io::Error::last_os_error());
                }
                handlers.previous.push(PreviousSignalAction {
                    signal,
                    action: previous,
                });
            }
            Ok(handlers)
        }

        fn take_count(&self) -> usize {
            SIGNAL_COUNT.swap(0, Ordering::AcqRel)
        }

        fn first_signal(&self) -> Option<i32> {
            match FIRST_SIGNAL.load(Ordering::Acquire) {
                0 => None,
                signal => Some(signal),
            }
        }

        fn restore(&mut self) -> io::Result<()> {
            let mut failed = Vec::new();
            let mut first_error = None;
            while let Some(previous) = self.previous.pop() {
                // SAFETY: the saved action was returned by sigaction for this
                // exact signal and remains initialized.
                if unsafe { libc::sigaction(previous.signal, &previous.action, ptr::null_mut()) }
                    != 0
                {
                    if first_error.is_none() {
                        first_error = Some(io::Error::last_os_error());
                    }
                    failed.push(previous);
                }
            }
            failed.reverse();
            self.previous = failed;
            match first_error {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }
    }

    impl Drop for TemporarySignalHandlers {
        fn drop(&mut self) {
            let _ = self.restore();
        }
    }

    fn deadline_from(now: Instant, duration: Duration) -> Instant {
        now.checked_add(duration).unwrap_or(now)
    }

    fn next_wake(
        timeout_at: Instant,
        term_deadline: Option<Instant>,
        kill_deadline: Option<Instant>,
    ) -> Instant {
        let now = Instant::now();
        let phase_deadline = kill_deadline.or(term_deadline).unwrap_or(timeout_at);
        deadline_from(now, SUPERVISION_TICK).min(phase_deadline)
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
                        return Ok(true);
                    }
                }
                Err(Errno::INTR) => continue,
                Err(Errno::AGAIN) => return Ok(true),
                Err(error) => return Err(os_error(error)),
            }
        }
    }

    fn peek_leader(process: Pid) -> Result<Option<LeaderObservation>, io::Error> {
        let options = WaitidOptions::EXITED | WaitidOptions::NOHANG | WaitidOptions::NOWAIT;
        let status = waitid(WaitId::Pid(process), options).map_err(os_error)?;
        Ok(status.and_then(|status| {
            if status.exited() {
                Some(LeaderObservation::Exited)
            } else {
                status
                    .terminating_signal()
                    .and_then(|signal| i32::try_from(signal).ok())
                    .map(LeaderObservation::Signaled)
            }
        }))
    }

    fn reap_if_ready<O: ProcessObserver>(
        child: &mut ArmedChild,
        observer: &mut O,
    ) -> Result<(), io::Error> {
        if child.status().is_some() {
            return Ok(());
        }
        if let Some(status) = child.child_mut().try_wait()? {
            child.set_status(status);
            observer.leader_reaped();
        }
        Ok(())
    }

    fn reap_ready_leader<O: ProcessObserver>(
        child: &mut ArmedChild,
        observer: &mut O,
    ) -> Result<(), io::Error> {
        if child.status().is_some() {
            return Ok(());
        }
        let status = child.child_mut().wait()?;
        child.set_status(status);
        observer.leader_reaped();
        Ok(())
    }

    fn send_kill<O: ProcessObserver>(
        child: &mut ArmedChild,
        observer: &mut O,
        kill_sent: &mut bool,
        kill_deadline: &mut Option<Instant>,
        stdout: &CaptureBuffer,
        stderr: &CaptureBuffer,
    ) -> Result<(), ProcessError> {
        signal_group(child.process_group(), Signal::Kill)
            .map_err(|source| supervision_with_output(source, child, stdout, stderr))?;
        observer.kill_sent();
        let deadline = deadline_from(Instant::now(), FINAL_REAP_LIMIT);
        *kill_sent = true;
        child.mark_group_killed(deadline);
        *kill_deadline = Some(deadline);
        Ok(())
    }

    fn signal_group(process_group: Pid, signal: Signal) -> Result<(), io::Error> {
        match kill_process_group(process_group, signal) {
            Ok(()) | Err(Errno::SRCH) => Ok(()),
            Err(error) => Err(os_error(error)),
        }
    }

    fn process_group_exists(process_group: Pid) -> Result<bool, io::Error> {
        match test_kill_process_group(process_group) {
            Ok(()) => Ok(true),
            Err(Errno::SRCH) => Ok(false),
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

    fn termination_error(
        first_parent_signal: Option<i32>,
        cause: Option<TerminationCause>,
        timeout: Duration,
        child: &ArmedChild,
        stdout: &CaptureBuffer,
        stderr: &CaptureBuffer,
    ) -> ProcessError {
        let output = partial_output(child, stdout, stderr);
        if let Some(signal) = first_parent_signal {
            ProcessError::Interrupted {
                signal,
                output,
                restore: super::SignalRestoreGuard::disarmed(),
            }
        } else {
            match cause.expect("termination has a cause") {
                TerminationCause::Timeout => ProcessError::TimedOut { timeout, output },
                TerminationCause::ChildSignal(signal) => {
                    ProcessError::ChildSignaled { signal, output }
                }
            }
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
        cleanup_deadline: Option<Instant>,
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
                cleanup_deadline: None,
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

        fn mark_group_killed(&mut self, cleanup_deadline: Instant) {
            self.group_killed = true;
            self.cleanup_deadline = Some(cleanup_deadline);
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

            // If normal supervision already spent the final-reap budget, Drop
            // must not silently begin a second five-second window.
            let deadline = self
                .cleanup_deadline
                .unwrap_or_else(|| deadline_from(Instant::now(), FINAL_REAP_LIMIT));
            loop {
                match self.child.try_wait() {
                    Ok(Some(status)) => {
                        self.status = Some(status);
                        return;
                    }
                    Ok(None) if Instant::now() < deadline => {
                        let wake = deadline_from(Instant::now(), SUPERVISION_TICK).min(deadline);
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

    #[cfg(test)]
    mod capture_tests {
        use super::*;

        #[test]
        fn exact_limit_is_preserved() {
            let bytes: Vec<u8> = (0..CAPTURE_LIMIT_BYTES).map(|n| (n % 251) as u8).collect();
            let mut capture = CaptureBuffer::new(CAPTURE_LIMIT_BYTES);
            capture.push(&bytes).unwrap();
            let result = capture.finish();
            assert_eq!(result.bytes, bytes);
            assert_eq!(result.original_bytes, CAPTURE_LIMIT_BYTES as u64);
            assert!(!result.truncated);
        }

        #[test]
        fn maximum_plus_one_retains_equal_head_and_tail() {
            let bytes: Vec<u8> = (0..=CAPTURE_LIMIT_BYTES).map(|n| (n % 251) as u8).collect();
            let mut capture = CaptureBuffer::new(CAPTURE_LIMIT_BYTES);
            capture.push(&bytes).unwrap();
            let result = capture.finish();
            let half = CAPTURE_LIMIT_BYTES / 2;
            assert_eq!(&result.bytes[..half], &bytes[..half]);
            assert_eq!(&result.bytes[half..], &bytes[bytes.len() - half..]);
            assert_eq!(result.original_bytes, bytes.len() as u64);
            assert!(result.truncated);
            assert!(result
                .render_lossy()
                .contains("... 1 bytes omitted from bounded process output ..."));
        }

        #[test]
        fn setup_failure_preserves_a_signal_captured_before_finalization() {
            let handlers = TemporarySignalHandlers::install().unwrap();
            capture_termination_signal(SIGINT);
            let result = finalize_signal_handlers(
                Err(ProcessError::Spawn(io::Error::from_raw_os_error(
                    libc::EINVAL,
                ))),
                handlers,
            );
            assert!(matches!(
                result,
                Err(ProcessError::Interrupted { signal, .. }) if signal == SIGINT
            ));
        }
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::{
        run, run_observed, ProcessError, ProcessLimits, ProcessTestEvent, CAPTURE_LIMIT_BYTES,
    };
    use rustix::fs::{flock, FlockOperation};
    use rustix::process::{kill_process, kill_process_group, Pid, Signal};
    use std::fs::{File, OpenOptions};
    use std::io::{self, BufRead, BufReader, Read, Write};
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::path::{Path, PathBuf};
    use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{mpsc, OnceLock};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use std::time::Instant;
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
    static HARNESS_SERIAL: OnceLock<Mutex<()>> = OnceLock::new();

    fn command(script: &str) -> Command {
        let mut command = Command::new("bash");
        command.arg("-c").arg(script);
        command
    }

    fn short_limits(timeout: Duration) -> ProcessLimits {
        ProcessLimits {
            timeout,
            term_grace: Duration::from_millis(100),
        }
    }

    #[test]
    fn successful_process_preserves_exact_output_and_receives_eof() {
        let result = run(
            command(
                "if IFS= read -r value; then exit 91; fi; printf 'stdout'; printf '\\377stderr' >&2",
            ),
            short_limits(Duration::from_secs(2)),
        )
        .unwrap();
        assert!(result.status.success());
        assert_eq!(result.stdout.bytes, b"stdout");
        assert_eq!(result.stderr.bytes, b"\xffstderr");
        assert_eq!(result.stderr.render_lossy(), "�stderr");
    }

    #[test]
    fn ordinary_nonzero_exit_is_a_completed_process() {
        let result = run(
            command("printf 'before'; printf 'error' >&2; exit 23"),
            short_limits(Duration::from_secs(2)),
        )
        .unwrap();
        assert_eq!(result.status.code(), Some(23));
        assert_eq!(result.stdout.bytes, b"before");
        assert_eq!(result.stderr.bytes, b"error");
    }

    #[test]
    fn spawn_failure_is_distinct() {
        let result = run(
            Command::new("/definitely/not/a/checksy-command"),
            short_limits(Duration::from_secs(2)),
        );
        assert!(matches!(result, Err(ProcessError::Spawn(_))));
    }

    #[test]
    fn child_signal_is_distinct_and_retains_output() {
        let result = run(
            command("printf 'before-signal'; kill -TERM $$"),
            short_limits(Duration::from_secs(2)),
        );
        match result {
            Err(ProcessError::ChildSignaled { signal, output }) => {
                assert_eq!(signal, signal_hook::consts::signal::SIGTERM);
                assert_eq!(output.stdout.bytes, b"before-signal");
            }
            other => panic!("expected child signal, got {other:?}"),
        }
    }

    #[test]
    fn timeout_retains_partial_output_and_escalates_for_resistant_process() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let observed = events.clone();
        let result = run_observed(
            command(
                "trap '' TERM; printf 'partial-out'; printf 'partial-err' >&2; while :; do :; done",
            ),
            short_limits(Duration::from_millis(100)),
            move |event| observed.lock().unwrap().push(event),
        );
        match result {
            Err(ProcessError::TimedOut { output, .. }) => {
                assert_eq!(output.stdout.bytes, b"partial-out");
                assert_eq!(output.stderr.bytes, b"partial-err");
            }
            other => panic!("expected timeout, got {other:?}"),
        }
        let events = events.lock().unwrap();
        assert!(matches!(
            events.first(),
            Some(ProcessTestEvent::Spawned { .. })
        ));
        assert!(events.contains(&ProcessTestEvent::TermSent));
        assert!(events.contains(&ProcessTestEvent::KillSent));
        assert!(matches!(
            events.last(),
            Some(ProcessTestEvent::LeaderReaped)
        ));
    }

    #[test]
    fn term_cooperative_timeout_does_not_need_kill() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let observed = events.clone();
        let result = run_observed(
            command("trap 'exit 0' TERM; printf 'ready'; while :; do sleep 10; done"),
            short_limits(Duration::from_millis(250)),
            move |event| observed.lock().unwrap().push(event),
        );
        assert!(matches!(result, Err(ProcessError::TimedOut { .. })));
        let events = events.lock().unwrap();
        assert!(events.contains(&ProcessTestEvent::TermSent));
        assert!(!events.contains(&ProcessTestEvent::KillSent));
        assert!(matches!(
            events.last(),
            Some(ProcessTestEvent::LeaderReaped)
        ));
    }

    #[test]
    fn drains_large_stdout_and_stderr_independently() {
        let script = format!(
            "(head -c {} /dev/zero | tr '\\0' O) & \
             (head -c {} /dev/zero | tr '\\0' E >&2) & wait",
            CAPTURE_LIMIT_BYTES * 2,
            CAPTURE_LIMIT_BYTES * 2
        );
        let result = run(command(&script), short_limits(Duration::from_secs(5))).unwrap();

        assert!(result.status.success());
        for (captured, expected) in [(&result.stdout, b'O'), (&result.stderr, b'E')] {
            assert_eq!(captured.original_bytes, (CAPTURE_LIMIT_BYTES * 2) as u64);
            assert_eq!(captured.bytes.len(), CAPTURE_LIMIT_BYTES);
            assert!(captured.truncated);
            assert!(captured.bytes.iter().all(|byte| *byte == expected));
        }
    }

    #[test]
    fn continuous_writers_cannot_starve_the_deadline() {
        let started = Instant::now();
        let result = run(
            command(
                "trap '' TERM; while :; do printf 0123456789abcdef; printf fedcba9876543210 >&2; done",
            ),
            ProcessLimits {
                timeout: Duration::from_millis(100),
                term_grace: Duration::from_millis(50),
            },
        );

        assert!(matches!(&result, Err(ProcessError::TimedOut { .. })));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "continuous writers delayed timeout for {:?}",
            started.elapsed()
        );
        let output = result.unwrap_err().output().unwrap().clone();
        assert!(output.stdout.original_bytes > 0);
        assert!(output.stderr.original_bytes > 0);
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

    fn term_ignoring_harness_command(root: &Path, nonce: &str) -> Command {
        let helper = harness_command("leader", root, nonce);
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
        // SAFETY: every caller uses a nonzero PID reported by a live Child or
        // by the process supervisor's spawn observer.
        unsafe { Pid::from_raw(raw_pid) }.expect("test process ID cannot be zero")
    }

    fn pause_until(deadline: Instant) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        let millis = i32::try_from(remaining.as_millis().clamp(1, 25)).unwrap_or(25);
        let _ = rustix::io::poll(&mut [], millis);
    }

    fn try_wait_until(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
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

    fn lock_path(root: &Path, role: &str) -> PathBuf {
        root.join(format!("{role}.lock"))
    }

    fn hold_lock(path: &Path) -> File {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .unwrap_or_else(|error| panic!("open {}: {error}", path.display()));
        flock(&file, FlockOperation::LockExclusive)
            .unwrap_or_else(|error| panic!("lock {}: {error}", path.display()));
        file
    }

    fn assert_lock_immediately_reacquirable(path: &Path) {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap_or_else(|error| panic!("reopen {}: {error}", path.display()));
        flock(&file, FlockOperation::NonBlockingLockExclusive)
            .unwrap_or_else(|error| panic!("{} remained locked: {error}", path.display()));
    }

    fn wait_for_nonce_line(stdout: ChildStdout, marker: String) -> String {
        let (sender, receiver) = mpsc::channel();
        let thread_marker = marker.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => return,
                    Ok(_) if line.contains(&thread_marker) => {
                        let _ = sender.send(line);
                        return;
                    }
                    Ok(_) => {}
                }
            }
        });
        receiver
            .recv_timeout(READINESS_TIMEOUT)
            .unwrap_or_else(|_| panic!("readiness marker '{marker}' timed out"))
    }

    fn park_forever() -> ! {
        loop {
            thread::park();
        }
    }

    struct HelperChild(Child);

    impl HelperChild {
        fn spawn(command: &mut Command) -> Self {
            Self(command.spawn().unwrap())
        }

        fn id(&self) -> u32 {
            self.0.id()
        }

        fn take_stdout(&mut self) -> ChildStdout {
            self.0.stdout.take().unwrap()
        }
    }

    impl Drop for HelperChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    enum WatchdogMessage {
        Spawned(u32),
        Done,
    }

    fn inner_watchdog(receiver: mpsc::Receiver<WatchdogMessage>) -> thread::JoinHandle<bool> {
        thread::spawn(move || {
            let deadline = Instant::now() + INNER_WATCHDOG;
            let mut process_group = None;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                match receiver.recv_timeout(remaining) {
                    Ok(WatchdogMessage::Spawned(group)) => process_group = Some(group),
                    Ok(WatchdogMessage::Done) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return false;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if let Some(group) = process_group {
                            let _ = kill_process_group(pid_from_u32(group), Signal::Kill);
                        }
                        return true;
                    }
                }
            }
        })
    }

    fn run_tree_scenario(root: &Path, nonce: &str) {
        let (watchdog_sender, watchdog_receiver) = mpsc::channel();
        let watchdog = inner_watchdog(watchdog_receiver);
        let mut events = Vec::new();
        let command = term_ignoring_harness_command(root, nonce);
        let limits = ProcessLimits {
            timeout: Duration::from_secs(3),
            term_grace: Duration::from_millis(100),
        };
        let result = run_observed(command, limits, |event| {
            events.push(event);
            if let ProcessTestEvent::Spawned { process_group } = event {
                println!("RUNNER_PGID:{nonce}:{process_group}");
                io::stdout().flush().unwrap();
                let _ = watchdog_sender.send(WatchdogMessage::Spawned(process_group));
            }
        });
        let _ = watchdog_sender.send(WatchdogMessage::Done);
        assert!(!watchdog.join().unwrap(), "inner watchdog fired");

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
        assert!(stdout.contains(&format!("PRE_TIMEOUT_STDOUT:{nonce}")));
        assert!(stderr.contains(&format!("PRE_TIMEOUT_STDERR:{nonce}")));

        let marker = format!("TREE_READY:{nonce}:");
        let line = stdout
            .lines()
            .find(|line| line.contains(&marker))
            .expect("leader did not report complete tree readiness");
        let suffix = line.split(&marker).nth(1).unwrap();
        let pids: Vec<u32> = suffix
            .split(':')
            .take(3)
            .map(|value| value.trim().parse().unwrap())
            .collect();
        assert_eq!(pids.len(), 3);
        assert_eq!(
            events,
            vec![
                ProcessTestEvent::Spawned {
                    process_group: pids[0]
                },
                ProcessTestEvent::TermSent,
                ProcessTestEvent::KillSent,
                ProcessTestEvent::LeaderReaped,
            ]
        );

        assert_lock_immediately_reacquirable(&lock_path(root, "leader"));
        assert_lock_immediately_reacquirable(&lock_path(root, "child"));
        assert_lock_immediately_reacquirable(&lock_path(root, "grandchild"));
    }

    fn run_second_signal_scenario(nonce: &str) {
        let (watchdog_sender, watchdog_receiver) = mpsc::channel();
        let watchdog = inner_watchdog(watchdog_receiver);
        let mut events = Vec::new();
        let started = Instant::now();
        let result = run_observed(
            command("trap '' INT TERM HUP QUIT; while :; do :; done"),
            ProcessLimits {
                timeout: Duration::from_secs(10),
                term_grace: Duration::from_secs(5),
            },
            |event| {
                events.push(event);
                match event {
                    ProcessTestEvent::Spawned { process_group } => {
                        println!("RUNNER_PGID:{nonce}:{process_group}");
                        io::stdout().flush().unwrap();
                        let _ = watchdog_sender.send(WatchdogMessage::Spawned(process_group));
                        kill_process(rustix::process::getpid(), Signal::Int).unwrap();
                    }
                    ProcessTestEvent::ParentSignalForwarded { signal } => {
                        assert_eq!(signal, Signal::Int as i32);
                        kill_process(rustix::process::getpid(), Signal::Int).unwrap();
                    }
                    ProcessTestEvent::TermSent
                    | ProcessTestEvent::KillSent
                    | ProcessTestEvent::LeaderReaped => {}
                }
            },
        );
        let _ = watchdog_sender.send(WatchdogMessage::Done);
        assert!(!watchdog.join().unwrap(), "inner watchdog fired");

        match result {
            Err(ProcessError::Interrupted { signal, .. }) => {
                assert_eq!(signal, Signal::Int as i32)
            }
            other => panic!("expected parent interruption, got {other:?}"),
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "second signal did not bypass the five-second grace: {:?}",
            started.elapsed()
        );
        assert!(events.contains(&ProcessTestEvent::KillSent));
        assert!(matches!(
            events.last(),
            Some(ProcessTestEvent::LeaderReaped)
        ));
    }

    fn run_completion_signal_scenario() {
        let result = run_observed(
            command("true"),
            short_limits(Duration::from_secs(2)),
            |event| {
                if event == ProcessTestEvent::LeaderReaped {
                    signal_hook::low_level::raise(signal_hook::consts::SIGINT).unwrap();
                }
            },
        );
        match result {
            Err(ProcessError::Interrupted { signal, output, .. }) => {
                assert_eq!(signal, Signal::Int as i32);
                assert!(output.status.is_some_and(|status| status.success()));
            }
            other => panic!("completion SIGINT was not retained: {other:?}"),
        }
    }

    struct IsolatedChild {
        child: Option<Child>,
        helper_group: Pid,
        inner_group: Arc<Mutex<Option<Pid>>>,
    }

    impl IsolatedChild {
        fn cleanup(&mut self) {
            let Some(mut child) = self.child.take() else {
                return;
            };
            if let Some(group) = *self.inner_group.lock().unwrap() {
                let _ = kill_process_group(group, Signal::Kill);
            }
            let _ = kill_process_group(self.helper_group, Signal::Kill);
            let _ = child.kill();
            let _ = try_wait_until(&mut child, Instant::now() + Duration::from_secs(1));
        }

        fn disarm(&mut self) {
            self.child.take();
        }
    }

    impl Drop for IsolatedChild {
        fn drop(&mut self) {
            self.cleanup();
        }
    }

    fn collect_pipe(
        pipe: impl Read + Send + 'static,
        nonce: String,
        inner_group: Arc<Mutex<Option<Pid>>>,
    ) -> mpsc::Receiver<Vec<u8>> {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let marker = format!("RUNNER_PGID:{nonce}:");
            let mut reader = BufReader::new(pipe);
            let mut bytes = Vec::new();
            loop {
                let mut line = Vec::new();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let text = String::from_utf8_lossy(&line);
                        if let Some(raw) = text
                            .split(&marker)
                            .nth(1)
                            .and_then(|value| value.trim().parse::<u32>().ok())
                        {
                            *inner_group.lock().unwrap() = Some(pid_from_u32(raw));
                        }
                        bytes.extend_from_slice(&line);
                    }
                }
            }
            let _ = sender.send(bytes);
        });
        receiver
    }

    fn isolated_scenario_result(mode: &str) -> (ExitStatus, Vec<u8>, Vec<u8>) {
        let _serial = HARNESS_SERIAL
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap();
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
        let inner_group = Arc::new(Mutex::new(None));
        let mut child = IsolatedChild {
            child: Some(child),
            helper_group,
            inner_group: inner_group.clone(),
        };
        let stdout = collect_pipe(
            child.child.as_mut().unwrap().stdout.take().unwrap(),
            nonce,
            inner_group,
        );
        let stderr = collect_pipe(
            child.child.as_mut().unwrap().stderr.take().unwrap(),
            String::new(),
            Arc::new(Mutex::new(None)),
        );

        let status = match try_wait_until(
            child.child.as_mut().unwrap(),
            Instant::now() + OUTER_WATCHDOG,
        )
        .unwrap()
        {
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
        (status, stdout, stderr)
    }

    fn run_isolated_scenario(mode: &str) {
        let (status, stdout, stderr) = isolated_scenario_result(mode);
        assert!(
            status.success(),
            "isolated {mode} scenario failed with {status}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
    }

    #[test]
    fn term_resistant_leader_child_and_grandchild_are_killed_and_reaped() {
        run_isolated_scenario("scenario-tree");
    }

    #[test]
    fn second_parent_signal_forces_immediate_kill() {
        run_isolated_scenario("scenario-second-signal");
    }

    #[test]
    fn idle_termination_signal_uses_default_action_after_successful_run() {
        let (status, stdout, stderr) = isolated_scenario_result("scenario-idle-signal");
        assert_eq!(
            status.signal(),
            Some(Signal::Int as i32),
            "idle signal scenario did not terminate by SIGINT: {status}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
    }

    #[test]
    fn preexisting_custom_signal_handler_is_restored_after_successful_run() {
        run_isolated_scenario("scenario-custom-signal");
    }

    #[test]
    fn termination_signal_at_completion_is_not_lost() {
        run_isolated_scenario("scenario-completion-signal");
    }

    #[test]
    #[ignore = "subprocess helper invoked by deterministic process tests"]
    fn process_harness_helper() {
        let Ok(mode) = std::env::var(HARNESS_MODE) else {
            return;
        };
        let root = PathBuf::from(std::env::var_os(HARNESS_ROOT).expect("harness root"));
        let nonce = std::env::var(HARNESS_NONCE).expect("harness nonce");

        match mode.as_str() {
            "scenario-tree" => run_tree_scenario(&root, &nonce),
            "scenario-second-signal" => run_second_signal_scenario(&nonce),
            "scenario-idle-signal" => {
                let result = run(command("true"), short_limits(Duration::from_secs(2))).unwrap();
                assert!(result.status.success());
                kill_process(rustix::process::getpid(), Signal::Int).unwrap();
                panic!("SIGINT was swallowed after the supervised command completed");
            }
            "scenario-custom-signal" => {
                let handled = Arc::new(AtomicBool::new(false));
                let _registration =
                    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&handled))
                        .unwrap();
                let result = run(command("true"), short_limits(Duration::from_secs(2))).unwrap();
                assert!(result.status.success());
                kill_process(rustix::process::getpid(), Signal::Int).unwrap();
                let deadline = Instant::now() + Duration::from_secs(1);
                while !handled.load(Ordering::Acquire) && Instant::now() < deadline {
                    pause_until(deadline);
                }
                assert!(
                    handled.load(Ordering::Acquire),
                    "the preexisting SIGINT handler was not restored"
                );
            }
            "scenario-completion-signal" => run_completion_signal_scenario(),
            "leader" => {
                let _leader_lock = hold_lock(&lock_path(&root, "leader"));
                let leader_pid = std::process::id();
                assert_eq!(pid_as_u32(rustix::process::getpgrp()), leader_pid);
                let mut command = harness_command("child", &root, &nonce);
                command
                    .env(HARNESS_PGID, leader_pid.to_string())
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());
                let mut child = HelperChild::spawn(&mut command);
                let child_pid = child.id();
                let line =
                    wait_for_nonce_line(child.take_stdout(), format!("CHILD_READY:{nonce}:"));
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
                io::stdout().flush().unwrap();
                io::stderr().flush().unwrap();
                park_forever();
            }
            "child" => {
                let expected_group: u32 = std::env::var(HARNESS_PGID).unwrap().parse().unwrap();
                assert_eq!(pid_as_u32(rustix::process::getpgrp()), expected_group);
                let _child_lock = hold_lock(&lock_path(&root, "child"));
                let child_pid = std::process::id();
                let mut command = harness_command("grandchild", &root, &nonce);
                command
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());
                let mut grandchild = HelperChild::spawn(&mut command);
                let grandchild_pid = grandchild.id();
                wait_for_nonce_line(
                    grandchild.take_stdout(),
                    format!("GRANDCHILD_READY:{nonce}:{grandchild_pid}"),
                );
                println!("CHILD_READY:{nonce}:{child_pid}:{grandchild_pid}");
                io::stdout().flush().unwrap();
                park_forever();
            }
            "grandchild" => {
                let expected_group: u32 = std::env::var(HARNESS_PGID).unwrap().parse().unwrap();
                assert_eq!(pid_as_u32(rustix::process::getpgrp()), expected_group);
                let _grandchild_lock = hold_lock(&lock_path(&root, "grandchild"));
                println!("GRANDCHILD_READY:{nonce}:{}", std::process::id());
                io::stdout().flush().unwrap();
                park_forever();
            }
            other => panic!("unknown process harness mode: {other}"),
        }
    }
}
