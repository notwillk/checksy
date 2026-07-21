use std::fmt;
use std::path::{Path, PathBuf};

/// An exclusive operating-system lock for one complete state directory.
///
/// The descriptor is deliberately kept private and open for the guard's whole
/// lifetime. Dropping the guard closes it and releases the advisory lock.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[derive(Debug)]
pub(crate) struct StateDirectoryLock {
    _lock_fd: rustix::fd::OwnedFd,
    canonical_root: PathBuf,
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[derive(Debug)]
pub(crate) struct StateDirectoryLock;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LockError {
    Held,
    State(String),
    #[allow(dead_code)] // Constructed by `acquire` on unsupported target builds.
    UnsupportedPlatform,
}

impl fmt::Display for LockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Held => formatter.write_str("state directory lock is already held"),
            Self::State(message) => formatter.write_str(message),
            Self::UnsupportedPlatform => {
                formatter.write_str("state directory locking is supported only on Linux and macOS")
            }
        }
    }
}

impl std::error::Error for LockError {}

impl StateDirectoryLock {
    /// Try to acquire the state directory's exclusive lock without waiting.
    ///
    /// This compatibility entry point prepares and canonicalizes an
    /// operator-selected legacy cache/state root. The future hardened state
    /// store will validate the complete root path before calling the same
    /// descriptor-relative lock-file implementation.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn acquire(state_dir: &Path) -> Result<Self, LockError> {
        supported::acquire(state_dir).map(|(lock_fd, canonical_root)| Self {
            _lock_fd: lock_fd,
            canonical_root,
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(crate) fn acquire(_state_dir: &Path) -> Result<Self, LockError> {
        Err(LockError::UnsupportedPlatform)
    }

    /// Acquire the persistent lock beneath an already opened and validated
    /// state root.
    ///
    /// The hardened state store uses this entry point so path validation and
    /// locking refer to the same directory inode. The legacy path-based entry
    /// point intentionally keeps its existing symlink-compatible behavior.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn acquire_opened(
        root_fd: &impl rustix::fd::AsFd,
        display_path: &Path,
    ) -> Result<Self, LockError> {
        supported::acquire_opened(root_fd, display_path).map(|lock_fd| Self {
            _lock_fd: lock_fd,
            canonical_root: display_path.to_path_buf(),
        })
    }

    /// The canonical directory selected before the lock entry was opened.
    ///
    /// Mutation callers reuse this path after acquisition so a trusted legacy
    /// cache-root symlink cannot redirect their work to a different namespace
    /// while they continue to hold the original directory's lock.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn canonical_root(&self) -> &Path {
        &self.canonical_root
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(crate) fn canonical_root(&self) -> &Path {
        unreachable!("an unsupported platform cannot construct a state-directory lock")
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod supported {
    use super::LockError;
    use rustix::fd::OwnedFd;
    use rustix::fs::{self, FileType, FlockOperation, Mode, OFlags};
    use rustix::io::{retry_on_intr, Errno};
    use rustix::process::{getegid, geteuid};
    use std::path::{Path, PathBuf};

    const LOCK_FILE_NAME: &str = "lock";

    pub(super) fn acquire(state_dir: &Path) -> Result<(OwnedFd, PathBuf), LockError> {
        std::fs::create_dir_all(state_dir).map_err(|error| {
            LockError::State(format!(
                "failed to create state directory '{}': {error}",
                state_dir.display()
            ))
        })?;

        let canonical_root = std::fs::canonicalize(state_dir).map_err(|error| {
            LockError::State(format!(
                "failed to resolve state directory '{}': {error}",
                state_dir.display()
            ))
        })?;
        let root_fd = fs::openat(
            fs::cwd(),
            &canonical_root,
            OFlags::RDONLY
                | OFlags::DIRECTORY
                | OFlags::NOFOLLOW
                | OFlags::NONBLOCK
                | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| state_io_error("open", &canonical_root, error))?;

        let lock_fd = acquire_opened(&root_fd, &canonical_root)?;
        Ok((lock_fd, canonical_root))
    }

    pub(super) fn acquire_opened(
        root_fd: &impl rustix::fd::AsFd,
        display_path: &Path,
    ) -> Result<OwnedFd, LockError> {
        let lock_path = display_path.join(LOCK_FILE_NAME);
        let lock_fd = open_lock_file(root_fd, &lock_path)?;
        validate_lock_file(&lock_fd, &lock_path)?;

        match retry_on_intr(|| fs::flock(&lock_fd, FlockOperation::NonBlockingLockExclusive)) {
            Ok(()) => {}
            Err(error) if error == Errno::AGAIN || error == Errno::WOULDBLOCK => {
                return Err(LockError::Held);
            }
            Err(error) => return Err(state_io_error("lock", &lock_path, error)),
        }

        validate_lock_file(&lock_fd, &lock_path)?;
        Ok(lock_fd)
    }

    fn open_lock_file(
        root_fd: &impl rustix::fd::AsFd,
        lock_path: &Path,
    ) -> Result<OwnedFd, LockError> {
        let open_flags = OFlags::RDWR | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
        let lock_mode = Mode::RUSR | Mode::WUSR;

        match fs::openat(
            root_fd,
            LOCK_FILE_NAME,
            open_flags | OFlags::CREATE | OFlags::EXCL,
            lock_mode,
        ) {
            Ok(lock_fd) => {
                // Creation modes are filtered by umask. Restore the exact
                // private mutable-file mode on the newly created inode only.
                fs::fchmod(&lock_fd, lock_mode)
                    .map_err(|error| state_io_error("set permissions on", lock_path, error))?;
                Ok(lock_fd)
            }
            Err(Errno::EXIST) => fs::openat(root_fd, LOCK_FILE_NAME, open_flags, Mode::empty())
                .map_err(|error| state_io_error("open", lock_path, error)),
            Err(error) => Err(state_io_error("create", lock_path, error)),
        }
    }

    fn validate_lock_file(lock_fd: &OwnedFd, lock_path: &Path) -> Result<(), LockError> {
        let metadata =
            fs::fstat(lock_fd).map_err(|error| state_io_error("inspect", lock_path, error))?;
        validate_lock_metadata(&metadata, lock_path)
    }

    fn validate_lock_metadata(metadata: &fs::Stat, lock_path: &Path) -> Result<(), LockError> {
        if FileType::from_raw_mode(metadata.st_mode) != FileType::RegularFile {
            return Err(LockError::State(format!(
                "state lock '{}' is not a regular file",
                lock_path.display()
            )));
        }
        if metadata.st_nlink != 1 {
            return Err(LockError::State(format!(
                "state lock '{}' must have exactly one hard link, found {}",
                lock_path.display(),
                metadata.st_nlink
            )));
        }

        let permission_mask =
            Mode::RWXU | Mode::RWXG | Mode::RWXO | Mode::SUID | Mode::SGID | Mode::SVTX;
        let permissions = Mode::from_raw_mode(metadata.st_mode) & permission_mask;
        let required_permissions = Mode::RUSR | Mode::WUSR;
        if permissions != required_permissions {
            return Err(LockError::State(format!(
                "state lock '{}' must have mode 0600, found {:04o}",
                lock_path.display(),
                permissions.bits()
            )));
        }

        let effective_uid = geteuid().as_raw();
        let effective_gid = getegid().as_raw();
        if metadata.st_uid != effective_uid || metadata.st_gid != effective_gid {
            return Err(LockError::State(format!(
                "state lock '{}' must be owned by effective uid {} and gid {}; found uid {} and gid {}",
                lock_path.display(),
                effective_uid,
                effective_gid,
                metadata.st_uid,
                metadata.st_gid
            )));
        }

        Ok(())
    }

    fn state_io_error(action: &str, path: &Path, error: Errno) -> LockError {
        LockError::State(format!(
            "failed to {action} state lock '{}': {error}",
            path.display()
        ))
    }

    #[cfg(test)]
    mod tests {
        use super::{acquire, fs, validate_lock_metadata};
        use std::fs::OpenOptions;

        #[test]
        fn rejects_lock_owned_by_a_different_identity() {
            let temp = tempfile::tempdir().unwrap();
            drop(acquire(temp.path()).unwrap());

            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(temp.path().join("lock"))
                .unwrap();
            let mut metadata = fs::fstat(&file).unwrap();
            metadata.st_uid = metadata.st_uid.wrapping_add(1);

            let error = validate_lock_metadata(&metadata, &temp.path().join("lock")).unwrap_err();
            assert!(error.to_string().contains("effective uid"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LockError, StateDirectoryLock};
    use std::fs;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};

    const HELPER_MODE: &str = "CHECKSY_STATE_LOCK_TEST_MODE";
    const HELPER_ROOT: &str = "CHECKSY_STATE_LOCK_TEST_ROOT";

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn creates_private_single_link_lock_file() {
        use std::os::unix::fs::MetadataExt;

        let temp = tempfile::tempdir().unwrap();
        let state_dir = temp.path().join("new").join("state");
        let _lock = StateDirectoryLock::acquire(&state_dir).unwrap();

        let metadata = fs::symlink_metadata(state_dir.join("lock")).unwrap();
        assert!(metadata.is_file());
        assert_eq!(metadata.mode() & 0o7777, 0o600);
        assert_eq!(metadata.uid(), rustix::process::geteuid().as_raw());
        assert_eq!(metadata.gid(), rustix::process::getegid().as_raw());
        assert_eq!(metadata.nlink(), 1);
        assert_eq!(metadata.len(), 0);
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn same_directory_contends_across_processes() {
        let temp = tempfile::tempdir().unwrap();
        let _lock = StateDirectoryLock::acquire(temp.path()).unwrap();

        let output = helper_command("try", temp.path()).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("CHECKSY_LOCK_HELD"));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn different_directories_do_not_contend() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");

        let _first_lock = StateDirectoryLock::acquire(&first).unwrap();
        let _second_lock = StateDirectoryLock::acquire(&second).unwrap();
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn canonical_root_is_frozen_when_a_legacy_alias_is_retargeted() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let alias = temp.path().join("cache");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        symlink(&first, &alias).unwrap();

        let lock = StateDirectoryLock::acquire(&alias).unwrap();
        assert_eq!(lock.canonical_root(), first.canonicalize().unwrap());

        fs::remove_file(&alias).unwrap();
        symlink(&second, &alias).unwrap();
        assert_eq!(lock.canonical_root(), first.canonicalize().unwrap());
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn dropping_guard_releases_lock() {
        let temp = tempfile::tempdir().unwrap();
        let lock = StateDirectoryLock::acquire(temp.path()).unwrap();
        drop(lock);

        StateDirectoryLock::acquire(temp.path()).unwrap();
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn killed_owner_leaves_reacquirable_stale_file() {
        let temp = tempfile::tempdir().unwrap();
        let (mut child, mut output) = spawn_blocking_helper("hold", temp.path());
        wait_for_line(&mut output, "CHECKSY_LOCKED");

        let contention = StateDirectoryLock::acquire(temp.path());
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(contention.unwrap_err(), LockError::Held);

        let _lock = StateDirectoryLock::acquire(temp.path()).unwrap();
        assert!(temp.path().join("lock").is_file());
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn lock_descriptor_is_closed_across_exec() {
        let temp = tempfile::tempdir().unwrap();
        // Run the fork/exec proof in an otherwise isolated test process. Other
        // tests spawn commands in parallel, and the short fork-before-exec
        // window can transiently inherit even a CLOEXEC descriptor and make an
        // immediate nonblocking reacquisition flaky in this parent harness.
        let output = helper_command("cloexec", temp.path()).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("CHECKSY_CLOEXEC"));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn stale_contents_are_not_used_as_a_pid_decision() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("lock");
        fs::write(&path, b"999999\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let _lock = StateDirectoryLock::acquire(temp.path()).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"999999\n");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn rejects_symlink_lock_without_touching_target() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        fs::write(&target, b"must survive\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&target, temp.path().join("lock")).unwrap();

        assert!(matches!(
            StateDirectoryLock::acquire(temp.path()),
            Err(LockError::State(_))
        ));
        assert_eq!(fs::read(&target).unwrap(), b"must survive\n");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn rejects_non_regular_lock_entry() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("lock")).unwrap();

        assert!(matches!(
            StateDirectoryLock::acquire(temp.path()),
            Err(LockError::State(_))
        ));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn rejects_socket_lock_entry() {
        use std::os::unix::net::UnixListener;

        let temp = tempfile::tempdir().unwrap();
        let _socket = UnixListener::bind(temp.path().join("lock")).unwrap();

        assert!(matches!(
            StateDirectoryLock::acquire(temp.path()),
            Err(LockError::State(_))
        ));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn rejects_incorrect_lock_mode() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("lock");
        fs::write(&path, []).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

        let error = StateDirectoryLock::acquire(temp.path()).unwrap_err();
        assert!(matches!(&error, LockError::State(message) if message.contains("mode 0600")));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn rejects_multiply_linked_lock_file() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("lock");
        fs::write(&path, []).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::hard_link(&path, temp.path().join("lock-alias")).unwrap();

        let error = StateDirectoryLock::acquire(temp.path()).unwrap_err();
        assert!(matches!(&error, LockError::State(message) if message.contains("one hard link")));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn held_and_state_errors_have_distinct_display_text() {
        assert_eq!(
            LockError::Held.to_string(),
            "state directory lock is already held"
        );
        assert_eq!(LockError::State("broken".into()).to_string(), "broken");
        assert_eq!(
            LockError::UnsupportedPlatform.to_string(),
            "state directory locking is supported only on Linux and macOS"
        );
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    #[test]
    fn unsupported_platform_fails_closed() {
        assert_eq!(
            StateDirectoryLock::acquire(Path::new("unused")).unwrap_err(),
            LockError::UnsupportedPlatform
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn helper_command(mode: &str, state_dir: &Path) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--ignored")
            .arg("--exact")
            .arg("state_lock::tests::state_lock_process_helper")
            .arg("--nocapture")
            .env(HELPER_MODE, mode)
            .env(HELPER_ROOT, state_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn spawn_blocking_helper(
        mode: &str,
        state_dir: &Path,
    ) -> (Child, BufReader<std::process::ChildStdout>) {
        let mut child = helper_command(mode, state_dir).spawn().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        (child, output)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn wait_for_line(output: &mut impl BufRead, expected: &str) {
        loop {
            let mut line = String::new();
            let bytes = output.read_line(&mut line).unwrap();
            assert_ne!(bytes, 0, "helper exited before emitting {expected}");
            if line.contains(expected) {
                return;
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    #[ignore = "subprocess helper invoked by state-lock tests"]
    fn state_lock_process_helper() {
        let mode = std::env::var(HELPER_MODE).expect("helper mode");
        let state_dir = PathBuf::from(std::env::var_os(HELPER_ROOT).expect("helper root"));

        match mode.as_str() {
            "try" => {
                assert_eq!(
                    StateDirectoryLock::acquire(&state_dir).unwrap_err(),
                    LockError::Held
                );
                println!("CHECKSY_LOCK_HELD");
            }
            "hold" => {
                let _lock = StateDirectoryLock::acquire(&state_dir).unwrap();
                println!("CHECKSY_LOCKED");
                std::io::stdout().flush().unwrap();
                let mut input = Vec::new();
                std::io::stdin().read_to_end(&mut input).unwrap();
            }
            "wait" => {
                println!("CHECKSY_READY");
                std::io::stdout().flush().unwrap();
                let mut input = Vec::new();
                std::io::stdin().read_to_end(&mut input).unwrap();
            }
            "cloexec" => {
                let lock = StateDirectoryLock::acquire(&state_dir).unwrap();
                let (mut child, mut output) = spawn_blocking_helper("wait", &state_dir);
                wait_for_line(&mut output, "CHECKSY_READY");

                drop(lock);
                let _replacement = StateDirectoryLock::acquire(&state_dir).unwrap();

                drop(child.stdin.take());
                assert!(child.wait().unwrap().success());
                println!("CHECKSY_CLOEXEC");
            }
            other => panic!("unknown helper mode: {other}"),
        }
    }
}
