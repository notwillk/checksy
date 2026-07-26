use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::flag;
use signal_hook::low_level::unregister;
use signal_hook::SigId;
use std::ffi::OsString;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const DRAIN_CHUNKS_PER_TURN: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Termination {
    Exited(i32),
    Signaled(i32),
    TimedOut,
    Interrupted(i32),
}

#[derive(Debug)]
pub(crate) struct ProcessResult {
    pub(crate) termination: Termination,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) duration: Duration,
}

#[derive(Debug)]
pub(crate) struct ProcessSpec {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<OsString>,
    pub(crate) cwd: PathBuf,
    pub(crate) timeout: Duration,
    pub(crate) term_grace: Duration,
    pub(crate) output_limit: usize,
}

impl ProcessSpec {
    pub(crate) fn new(
        program: &Path,
        args: impl IntoIterator<Item = impl Into<OsString>>,
        cwd: &Path,
        timeout: Duration,
        term_grace: Duration,
        output_limit: usize,
    ) -> Self {
        Self {
            program: program.to_path_buf(),
            args: args.into_iter().map(Into::into).collect(),
            cwd: cwd.to_path_buf(),
            timeout,
            term_grace,
            output_limit,
        }
    }
}

struct SignalGuard {
    value: Arc<AtomicUsize>,
    registrations: Vec<SigId>,
}

impl SignalGuard {
    fn install() -> Result<Self, String> {
        let value = Arc::new(AtomicUsize::new(0));
        let mut registrations = Vec::new();
        for signal in [SIGHUP, SIGINT, SIGTERM] {
            match flag::register_usize(signal, Arc::clone(&value), signal as usize) {
                Ok(registration) => registrations.push(registration),
                Err(error) => {
                    for registration in registrations {
                        unregister(registration);
                    }
                    return Err(format!("install signal {signal} handler: {error}"));
                }
            }
        }
        Ok(Self {
            value,
            registrations,
        })
    }
}

impl Drop for SignalGuard {
    fn drop(&mut self) {
        for registration in self.registrations.drain(..) {
            unregister(registration);
        }
    }
}

struct Drain<T> {
    stream: T,
    bytes: TailBuffer,
    eof: bool,
}

impl<T: Read + AsRawFd> Drain<T> {
    fn new(stream: T, limit: usize) -> Result<Self, String> {
        let descriptor = stream.as_raw_fd();
        let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
        if flags < 0 {
            return Err(format!("read pipe flags: {}", io::Error::last_os_error()));
        }
        if unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(format!(
                "make pipe nonblocking: {}",
                io::Error::last_os_error()
            ));
        }
        Ok(Self {
            stream,
            bytes: TailBuffer::new(limit),
            eof: false,
        })
    }

    fn drain(&mut self) -> Result<(), String> {
        let mut buffer = [0_u8; 8192];
        for _ in 0..DRAIN_CHUNKS_PER_TURN {
            match self.stream.read(&mut buffer) {
                Ok(0) => {
                    self.eof = true;
                    return Ok(());
                }
                Ok(count) => self.bytes.push(&buffer[..count]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(format!("drain child output: {error}")),
            }
        }
        Ok(())
    }
}

struct TailBuffer {
    bytes: Vec<u8>,
    limit: usize,
}

impl TailBuffer {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit),
            limit,
        }
    }

    fn push(&mut self, incoming: &[u8]) {
        if self.limit == 0 {
            return;
        }
        if incoming.len() >= self.limit {
            self.bytes.clear();
            self.bytes
                .extend_from_slice(&incoming[incoming.len() - self.limit..]);
            return;
        }
        let excess = self
            .bytes
            .len()
            .saturating_add(incoming.len())
            .saturating_sub(self.limit);
        if excess != 0 {
            self.bytes.drain(..excess);
        }
        self.bytes.extend_from_slice(incoming);
    }
}

pub(crate) fn run(spec: &ProcessSpec) -> Result<ProcessResult, String> {
    run_with_interrupt(spec, None)
}

fn run_with_interrupt(
    spec: &ProcessSpec,
    test_interrupt: Option<Arc<AtomicUsize>>,
) -> Result<ProcessResult, String> {
    let signals = SignalGuard::install()?;
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("HOME", "/root")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            libc::umask(0o077);
            Ok(())
        });
    }
    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|error| format!("spawn {}: {error}", spec.program.display()))?;
    let process_group = match i32::try_from(child.id()) {
        Ok(process_group) => process_group,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("child PID {} does not fit in i32", child.id()));
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = terminate_group(&mut child, process_group, spec.term_grace);
            return Err("child stdout pipe is unavailable".to_owned());
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = terminate_group(&mut child, process_group, spec.term_grace);
            return Err("child stderr pipe is unavailable".to_owned());
        }
    };
    let mut stdout = match Drain::<ChildStdout>::new(stdout, spec.output_limit) {
        Ok(stdout) => stdout,
        Err(error) => {
            let _ = terminate_group(&mut child, process_group, spec.term_grace);
            return Err(error);
        }
    };
    let mut stderr = match Drain::<ChildStderr>::new(stderr, spec.output_limit) {
        Ok(stderr) => stderr,
        Err(error) => {
            let _ = terminate_group(&mut child, process_group, spec.term_grace);
            return Err(error);
        }
    };

    let monitored: Result<Termination, String> = (|| loop {
        stdout.drain()?;
        stderr.drain()?;
        let signal = signals.value.load(Ordering::Relaxed);
        let signal = if signal == 0 {
            test_interrupt
                .as_ref()
                .map_or(0, |value| value.load(Ordering::Relaxed))
        } else {
            signal
        };
        if signal != 0 {
            terminate_group(&mut child, process_group, spec.term_grace)?;
            break Ok(Termination::Interrupted(signal as i32));
        }
        if started.elapsed() >= spec.timeout {
            terminate_group(&mut child, process_group, spec.term_grace)?;
            break Ok(Termination::TimedOut);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("wait for {}: {error}", spec.program.display()))?
        {
            cleanup_descendants(process_group, spec.term_grace)?;
            break Ok(match (status.code(), status.signal()) {
                (Some(code), _) => Termination::Exited(code),
                (None, Some(signal)) => Termination::Signaled(signal),
                _ => Termination::Signaled(0),
            });
        }
        thread::sleep(Duration::from_millis(10));
    })();
    let termination = match monitored {
        Ok(termination) => termination,
        Err(error) => {
            let cleanup = terminate_group(&mut child, process_group, spec.term_grace);
            return match cleanup {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; process cleanup also failed: {cleanup}")),
            };
        }
    };

    let drain_deadline = Instant::now() + Duration::from_millis(500);
    while (!stdout.eof || !stderr.eof) && Instant::now() < drain_deadline {
        stdout.drain()?;
        stderr.drain()?;
        if !stdout.eof || !stderr.eof {
            thread::sleep(Duration::from_millis(5));
        }
    }

    Ok(ProcessResult {
        termination,
        stdout: stdout.bytes.bytes,
        stderr: stderr.bytes.bytes,
        duration: started.elapsed(),
    })
}

fn terminate_group(child: &mut Child, process_group: i32, grace: Duration) -> Result<(), String> {
    signal_group(process_group, libc::SIGTERM)?;
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if child
            .try_wait()
            .map_err(|error| format!("wait for terminated child: {error}"))?
            .is_some()
            && !group_exists(process_group)
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    signal_group(process_group, libc::SIGKILL)?;
    child
        .wait()
        .map_err(|error| format!("reap killed child: {error}"))?;
    wait_for_group_exit(process_group, Duration::from_secs(1))
}

fn cleanup_descendants(process_group: i32, grace: Duration) -> Result<(), String> {
    if !group_exists(process_group) {
        return Ok(());
    }
    signal_group(process_group, libc::SIGTERM)?;
    let deadline = Instant::now() + grace;
    while group_exists(process_group) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if group_exists(process_group) {
        signal_group(process_group, libc::SIGKILL)?;
        wait_for_group_exit(process_group, Duration::from_secs(1))?;
    }
    Ok(())
}

fn wait_for_group_exit(process_group: i32, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while group_exists(process_group) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if group_exists(process_group) {
        Err(format!(
            "process group {process_group} remained after SIGKILL"
        ))
    } else {
        Ok(())
    }
}

fn group_exists(process_group: i32) -> bool {
    if unsafe { libc::kill(-process_group, 0) } == 0 {
        return true;
    }
    matches!(io::Error::last_os_error().raw_os_error(), Some(libc::EPERM))
}

fn signal_group(process_group: i32, signal: i32) -> Result<(), String> {
    if unsafe { libc::kill(-process_group, signal) } == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(format!(
            "send signal {signal} to process group {process_group}: {error}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{group_exists, run_with_interrupt, ProcessSpec, Termination};
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    fn script(contents: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "rulesyos-process-test-{}-{}",
            std::process::id(),
            super::super::tests::next_test_id()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("command");
        fs::write(&path, contents).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[test]
    fn process_has_fixed_environment_cwd_args_and_bounded_output() {
        let command = script(
            "#!/bin/sh\n\
             printf 'cwd=%s\\n' \"$PWD\"\n\
             env | sort\n\
             printf 'args=%s\\n' \"$*\"\n\
             printf '0123456789abcdef'\n\
             printf 'fedcba9876543210' >&2\n",
        );
        let spec = ProcessSpec::new(
            &command,
            [OsString::from("one"), OsString::from("two")],
            Path::new("/"),
            Duration::from_secs(2),
            Duration::from_millis(50),
            12,
        );
        let result = run_with_interrupt(&spec, None).unwrap();
        assert_eq!(result.termination, Termination::Exited(0));
        assert_eq!(result.stdout, b"456789abcdef");
        assert_eq!(result.stderr, b"ba9876543210");
    }

    #[test]
    fn timeout_terminates_the_process_group() {
        let command = script(
            "#!/bin/sh\n\
             trap '' TERM\n\
             printf '%s\\n' \"$$\" > \"$1\"\n\
             while :; do /bin/sleep 1; done\n",
        );
        let pid_file = command.with_extension("pid");
        let spec = ProcessSpec::new(
            &command,
            [OsString::from(pid_file.as_os_str())],
            Path::new("/"),
            Duration::from_millis(100),
            Duration::from_millis(50),
            128,
        );
        let result = run_with_interrupt(&spec, None).unwrap();
        assert_eq!(result.termination, Termination::TimedOut);
        assert!(result.duration < Duration::from_secs(2));
        let process_group = fs::read_to_string(pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(!group_exists(process_group));
    }

    #[test]
    fn continuous_output_cannot_starve_the_outer_deadline() {
        let command = script(
            "#!/bin/sh\n\
             trap '' TERM\n\
             while :; do\n\
             \x20 printf 'continuous-stdout-output'\n\
             \x20 printf 'continuous-stderr-output' >&2\n\
             done\n",
        );
        let spec = ProcessSpec::new(
            &command,
            std::iter::empty::<OsString>(),
            Path::new("/"),
            Duration::from_millis(100),
            Duration::from_millis(50),
            128,
        );
        let result = run_with_interrupt(&spec, None).unwrap();
        assert_eq!(result.termination, Termination::TimedOut);
        assert!(!result.stdout.is_empty());
        assert!(!result.stderr.is_empty());
        assert!(result.stdout.len() <= 128);
        assert!(result.stderr.len() <= 128);
        assert!(result.duration < Duration::from_secs(2));
    }

    #[test]
    fn cancellation_interrupts_and_cleans_up_the_process_group() {
        let command = script("#!/bin/sh\ntrap '' TERM\nwhile :; do /bin/sleep 1; done\n");
        let flag = Arc::new(AtomicUsize::new(0));
        let trigger = Arc::clone(&flag);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            trigger.store(libc::SIGTERM as usize, Ordering::Relaxed);
        });
        let spec = ProcessSpec::new(
            &command,
            std::iter::empty::<OsString>(),
            Path::new("/"),
            Duration::from_secs(2),
            Duration::from_millis(50),
            128,
        );
        let result = run_with_interrupt(&spec, Some(flag)).unwrap();
        assert_eq!(result.termination, Termination::Interrupted(libc::SIGTERM));
    }
}
