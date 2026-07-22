//! Private provisioning semaphore for complete `check --fix` operations.

use std::fmt;
use std::path::Path;

/// A held, process-local provisioning semaphore.
///
/// The operating-system lock is released when this value is dropped or when
/// the process exits. It is deliberately neither `Clone` nor public.
#[derive(Debug)]
pub(crate) struct ProvisioningLock {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[allow(dead_code)] // Retaining this field retains the RAII lock.
    inner: supported::HeldLock,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ProvisioningLockError {
    Held,
    State(String),
    #[allow(dead_code)]
    UnsupportedPlatform,
}

impl fmt::Display for ProvisioningLockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Held => write!(
                formatter,
                "provisioning lock held: another checksy check --fix is already running for this user"
            ),
            Self::State(message) => write!(formatter, "provisioning lock state failed: {message}"),
            Self::UnsupportedPlatform => write!(
                formatter,
                "provisioning locks are supported only on Linux and macOS"
            ),
        }
    }
}

impl std::error::Error for ProvisioningLockError {}

#[cfg(test)]
static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Serialize in-process lock tests that use temporary filesystems whose inode
/// numbers may be recycled aggressively under the parallel test runner.
#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    TEST_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl ProvisioningLock {
    pub(crate) fn acquire() -> Result<Self, ProvisioningLockError> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            supported::acquire().map(|inner| Self { inner })
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            Err(ProvisioningLockError::UnsupportedPlatform)
        }
    }

    /// Acquire a lock in an explicitly selected final Checksy directory.
    ///
    /// This is crate-private so command-level tests can exercise orchestration
    /// without modifying the invoking account's real provisioning namespace.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn acquire_at(
        lock_directory: &Path,
        expected_uid: u32,
    ) -> Result<Self, ProvisioningLockError> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            supported::acquire_at(lock_directory, expected_uid).map(|inner| Self { inner })
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = (lock_directory, expected_uid);
            Err(ProvisioningLockError::UnsupportedPlatform)
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod supported {
    use super::ProvisioningLockError;
    use rustix::fd::OwnedFd;
    use rustix::fs::{
        fchmod, flock, fstat, mkdirat, openat, statat, AtFlags, FlockOperation, Mode, OFlags,
    };
    use rustix::io::Errno;
    use rustix::process::geteuid;
    use std::collections::HashSet;
    use std::ffi::{CStr, OsStr, OsString};
    use std::fs;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::{Component, Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

    const DIRECTORY_MODE: u32 = 0o700;
    const LOCK_MODE: u32 = 0o600;
    const PERMISSION_MASK: u32 = 0o7777;
    // Darwin's libc exposes these through a narrower `mode_t` than Linux.
    #[allow(clippy::unnecessary_cast)]
    const FILE_TYPE_MASK: u32 = libc::S_IFMT as u32;
    #[allow(clippy::unnecessary_cast)]
    const DIRECTORY_TYPE: u32 = libc::S_IFDIR as u32;
    #[allow(clippy::unnecessary_cast)]
    const REGULAR_FILE_TYPE: u32 = libc::S_IFREG as u32;
    const DEFAULT_PASSWD_BUFFER: usize = 16 * 1024;
    const MAX_PASSWD_BUFFER: usize = 1024 * 1024;

    type Result<T> = std::result::Result<T, ProvisioningLockError>;

    #[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
    struct LockKey {
        device: u64,
        inode: u64,
    }

    static PROCESS_LOCKS: OnceLock<Mutex<HashSet<LockKey>>> = OnceLock::new();

    fn process_locks() -> &'static Mutex<HashSet<LockKey>> {
        PROCESS_LOCKS.get_or_init(|| Mutex::new(HashSet::new()))
    }

    #[derive(Debug)]
    pub(super) struct HeldLock {
        descriptor: Option<OwnedFd>,
        key: LockKey,
    }

    #[derive(Debug)]
    struct OpenedLockDirectory {
        descriptor: OwnedFd,
        parent: OwnedFd,
        name: OsString,
    }

    impl Drop for HeldLock {
        fn drop(&mut self) {
            // Close the descriptor before making the inode available to a
            // second acquisition in this process.
            drop(self.descriptor.take());
            if let Ok(mut locks) = process_locks().lock() {
                locks.remove(&self.key);
            }
        }
    }

    pub(super) fn acquire() -> Result<HeldLock> {
        let expected_uid = geteuid().as_raw();
        let (anchor, suffix) = production_location(expected_uid)?;
        let anchor = fs::canonicalize(&anchor).map_err(|error| {
            state(format!(
                "cannot resolve account-state anchor {}: {error}",
                anchor.display()
            ))
        })?;
        let anchor_descriptor = open_directory(&anchor, "account-state anchor")?;
        let directory = traverse_to_lock_directory(anchor_descriptor, &suffix, expected_uid)?;
        acquire_in_directory(directory, expected_uid)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn acquire_at(lock_directory: &Path, expected_uid: u32) -> Result<HeldLock> {
        if !lock_directory.is_absolute() {
            return Err(state(format!(
                "lock directory must be absolute: {}",
                lock_directory.display()
            )));
        }

        let name = lock_directory.file_name().ok_or_else(|| {
            state(format!(
                "lock directory has no final component: {}",
                lock_directory.display()
            ))
        })?;
        if name.as_bytes().is_empty() || name == OsStr::new(".") || name == OsStr::new("..") {
            return Err(state("lock directory has an invalid final component"));
        }
        let parent = lock_directory.parent().ok_or_else(|| {
            state(format!(
                "lock directory has no parent: {}",
                lock_directory.display()
            ))
        })?;
        let parent = fs::canonicalize(parent).map_err(|error| {
            state(format!(
                "cannot resolve lock-directory parent {}: {error}",
                parent.display()
            ))
        })?;
        let parent_descriptor = open_directory(&parent, "lock-directory parent")?;
        let directory_descriptor =
            open_or_create_directory(&parent_descriptor, name, expected_uid, true)?;
        let directory = OpenedLockDirectory {
            descriptor: directory_descriptor,
            parent: parent_descriptor,
            name: name.to_os_string(),
        };
        acquire_in_directory(directory, expected_uid)
    }

    fn production_location(expected_uid: u32) -> Result<(PathBuf, Vec<OsString>)> {
        if expected_uid == 0 {
            #[cfg(target_os = "linux")]
            return Ok((
                PathBuf::from("/"),
                ["var", "lib", "checksy"]
                    .into_iter()
                    .map(OsString::from)
                    .collect(),
            ));

            #[cfg(target_os = "macos")]
            return Ok((
                PathBuf::from("/"),
                ["Library", "Application Support", "checksy"]
                    .into_iter()
                    .map(OsString::from)
                    .collect(),
            ));
        }

        Ok(non_root_location(account_home(expected_uid)?))
    }

    fn non_root_location(account_home: PathBuf) -> (PathBuf, Vec<OsString>) {
        #[cfg(target_os = "linux")]
        let suffix = [".local", "state", "checksy"];
        #[cfg(target_os = "macos")]
        let suffix = ["Library", "Application Support", "checksy"];
        (
            account_home,
            suffix.into_iter().map(OsString::from).collect(),
        )
    }

    fn account_home(uid: u32) -> Result<PathBuf> {
        let recommended = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
        let mut capacity = if recommended > 0 {
            usize::try_from(recommended)
                .unwrap_or(MAX_PASSWD_BUFFER)
                .clamp(1, MAX_PASSWD_BUFFER)
        } else {
            DEFAULT_PASSWD_BUFFER
        };

        loop {
            let mut record = std::mem::MaybeUninit::<libc::passwd>::uninit();
            let mut result = std::ptr::null_mut();
            let mut buffer = vec![0_u8; capacity];
            let status = unsafe {
                libc::getpwuid_r(
                    uid as libc::uid_t,
                    record.as_mut_ptr(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    &mut result,
                )
            };

            if status == libc::ERANGE && capacity < MAX_PASSWD_BUFFER {
                capacity = capacity.saturating_mul(2).min(MAX_PASSWD_BUFFER);
                continue;
            }
            if status != 0 {
                return Err(state(format!(
                    "cannot resolve account home for uid {uid}: {}",
                    std::io::Error::from_raw_os_error(status)
                )));
            }
            if result.is_null() {
                return Err(state(format!("no account exists for uid {uid}")));
            }

            let record = unsafe { record.assume_init() };
            if record.pw_dir.is_null() {
                return Err(state(format!("account uid {uid} has no home directory")));
            }
            let bytes = unsafe { CStr::from_ptr(record.pw_dir) }.to_bytes();
            if bytes.is_empty() {
                return Err(state(format!(
                    "account uid {uid} has an empty home directory"
                )));
            }
            let home = PathBuf::from(OsString::from_vec(bytes.to_vec()));
            if !home.is_absolute() {
                return Err(state(format!(
                    "account uid {uid} has a non-absolute home directory: {}",
                    home.display()
                )));
            }
            return Ok(home);
        }
    }

    fn traverse_to_lock_directory(
        mut current: OwnedFd,
        suffix: &[OsString],
        expected_uid: u32,
    ) -> Result<OpenedLockDirectory> {
        if suffix.is_empty() {
            return Err(state("provisioning lock suffix is empty"));
        }

        for (index, component) in suffix.iter().enumerate() {
            if Path::new(component).components().count() != 1
                || !matches!(
                    Path::new(component).components().next(),
                    Some(Component::Normal(_))
                )
            {
                return Err(state(
                    "provisioning lock suffix contains an invalid component",
                ));
            }
            let final_component = index + 1 == suffix.len();
            let next =
                open_or_create_directory(&current, component, expected_uid, final_component)?;
            if final_component {
                return Ok(OpenedLockDirectory {
                    descriptor: next,
                    parent: current,
                    name: component.clone(),
                });
            }
            current = next;
        }
        unreachable!("the empty suffix was rejected above")
    }

    fn open_directory(path: &Path, description: &str) -> Result<OwnedFd> {
        let descriptor = openat(
            rustix::fs::cwd(),
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| {
            state(format!(
                "cannot open {description} {}: {error}",
                path.display()
            ))
        })?;
        let metadata = fstat(&descriptor)
            .map_err(|error| state(format!("cannot inspect {description}: {error}")))?;
        if file_type(metadata.st_mode) != DIRECTORY_TYPE {
            return Err(state(format!("{description} is not a directory")));
        }
        Ok(descriptor)
    }

    fn open_or_create_directory(
        parent: &OwnedFd,
        name: &OsStr,
        expected_uid: u32,
        exact_mode: bool,
    ) -> Result<OwnedFd> {
        let mut created = false;
        match mkdirat(parent, name, Mode::RWXU) {
            Ok(()) => created = true,
            Err(Errno::EXIST) => {}
            Err(error) => {
                return Err(state(format!(
                    "cannot create provisioning directory {}: {error}",
                    name.to_string_lossy()
                )))
            }
        }

        let descriptor = openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| {
            state(format!(
                "cannot open provisioning directory {}: {error}",
                name.to_string_lossy()
            ))
        })?;
        if created {
            fchmod(&descriptor, Mode::RWXU).map_err(|error| {
                state(format!(
                    "cannot set provisioning directory {} mode: {error}",
                    name.to_string_lossy()
                ))
            })?;
        }
        validate_directory(&descriptor, name, expected_uid, exact_mode)?;
        Ok(descriptor)
    }

    fn validate_directory(
        descriptor: &OwnedFd,
        name: &OsStr,
        expected_uid: u32,
        exact_mode: bool,
    ) -> Result<()> {
        let metadata = fstat(descriptor).map_err(|error| {
            state(format!(
                "cannot inspect provisioning directory {}: {error}",
                name.to_string_lossy()
            ))
        })?;
        if file_type(metadata.st_mode) != DIRECTORY_TYPE {
            return Err(state(format!(
                "provisioning directory {} is not a directory",
                name.to_string_lossy()
            )));
        }
        if exact_mode {
            if metadata.st_uid as u32 != expected_uid {
                return Err(state(format!(
                    "provisioning directory {} is owned by uid {}, expected uid {expected_uid}",
                    name.to_string_lossy(),
                    metadata.st_uid
                )));
            }
            let actual_mode = permissions(metadata.st_mode);
            if actual_mode != DIRECTORY_MODE {
                return Err(state(format!(
                    "provisioning directory {} has mode {actual_mode:04o}, expected 0700",
                    name.to_string_lossy()
                )));
            }
        }
        Ok(())
    }

    fn acquire_in_directory(directory: OpenedLockDirectory, expected_uid: u32) -> Result<HeldLock> {
        validate_directory_path(&directory, expected_uid)?;
        let (descriptor, created) = open_lock_file(&directory.descriptor)?;
        if created {
            fchmod(&descriptor, Mode::RUSR | Mode::WUSR)
                .map_err(|error| state(format!("cannot set provisioning lock mode: {error}")))?;
        }

        let metadata = validate_lock_file(&descriptor, expected_uid)?;
        validate_lock_path(&directory.descriptor, &metadata)?;
        let key = LockKey {
            device: metadata.st_dev as u64,
            inode: metadata.st_ino as u64,
        };

        {
            let mut locks = process_locks()
                .lock()
                .map_err(|_| state("process-local provisioning lock registry is poisoned"))?;
            if !locks.insert(key) {
                return Err(ProvisioningLockError::Held);
            }
        }

        let flock_result = loop {
            match flock(&descriptor, FlockOperation::NonBlockingLockExclusive) {
                Err(Errno::INTR) => continue,
                result => break result,
            }
        };
        if let Err(error) = flock_result {
            remove_process_lock(key);
            if error == Errno::AGAIN || error == Errno::WOULDBLOCK {
                return Err(ProvisioningLockError::Held);
            }
            return Err(state(format!("cannot acquire provisioning lock: {error}")));
        }

        let locked_metadata = match validate_lock_file(&descriptor, expected_uid) {
            Ok(metadata) => metadata,
            Err(error) => {
                drop(descriptor);
                remove_process_lock(key);
                return Err(error);
            }
        };
        if let Err(error) = validate_lock_path(&directory.descriptor, &locked_metadata) {
            drop(descriptor);
            remove_process_lock(key);
            return Err(error);
        }
        if let Err(error) = validate_directory_path(&directory, expected_uid) {
            drop(descriptor);
            remove_process_lock(key);
            return Err(error);
        }

        Ok(HeldLock {
            descriptor: Some(descriptor),
            key,
        })
    }

    fn validate_directory_path(directory: &OpenedLockDirectory, expected_uid: u32) -> Result<()> {
        validate_directory(&directory.descriptor, &directory.name, expected_uid, true)?;
        let opened = fstat(&directory.descriptor)
            .map_err(|error| state(format!("cannot inspect provisioning directory: {error}")))?;
        let current = statat(
            &directory.parent,
            &directory.name,
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(|error| {
            state(format!(
                "cannot re-inspect provisioning directory {}: {error}",
                directory.name.to_string_lossy()
            ))
        })?;
        if current.st_dev != opened.st_dev || current.st_ino != opened.st_ino {
            return Err(state(
                "provisioning directory path changed while the lock was being acquired",
            ));
        }
        if file_type(current.st_mode) != DIRECTORY_TYPE {
            return Err(state("provisioning directory path is not a directory"));
        }
        Ok(())
    }

    fn open_lock_file(directory: &OwnedFd) -> Result<(OwnedFd, bool)> {
        let create_flags = OFlags::RDWR
            | OFlags::CREATE
            | OFlags::EXCL
            | OFlags::NOFOLLOW
            | OFlags::NONBLOCK
            | OFlags::CLOEXEC;
        match openat(
            directory,
            "provision.lock",
            create_flags,
            Mode::RUSR | Mode::WUSR,
        ) {
            Ok(descriptor) => Ok((descriptor, true)),
            Err(Errno::EXIST) => openat(
                directory,
                "provision.lock",
                OFlags::RDWR | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map(|descriptor| (descriptor, false))
            .map_err(|error| state(format!("cannot open provisioning lock: {error}"))),
            Err(error) => Err(state(format!("cannot create provisioning lock: {error}"))),
        }
    }

    fn validate_lock_file(descriptor: &OwnedFd, expected_uid: u32) -> Result<rustix::fs::Stat> {
        let metadata =
            fstat(descriptor).map_err(|error| state(format!("cannot inspect lock: {error}")))?;
        if file_type(metadata.st_mode) != REGULAR_FILE_TYPE {
            return Err(state("provisioning lock is not a regular file"));
        }
        if metadata.st_nlink != 1 {
            return Err(state(format!(
                "provisioning lock has {} links, expected 1",
                metadata.st_nlink
            )));
        }
        if metadata.st_uid as u32 != expected_uid {
            return Err(state(format!(
                "provisioning lock is owned by uid {}, expected uid {expected_uid}",
                metadata.st_uid
            )));
        }
        let actual_mode = permissions(metadata.st_mode);
        if actual_mode != LOCK_MODE {
            return Err(state(format!(
                "provisioning lock has mode {actual_mode:04o}, expected 0600"
            )));
        }
        Ok(metadata)
    }

    fn validate_lock_path(directory: &OwnedFd, opened: &rustix::fs::Stat) -> Result<()> {
        let current = statat(directory, "provision.lock", AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|error| state(format!("cannot re-inspect provisioning lock path: {error}")))?;
        if current.st_dev != opened.st_dev || current.st_ino != opened.st_ino {
            return Err(state(
                "provisioning lock path changed while it was being opened",
            ));
        }
        if file_type(current.st_mode) != REGULAR_FILE_TYPE {
            return Err(state("provisioning lock path is not a regular file"));
        }
        Ok(())
    }

    fn remove_process_lock(key: LockKey) {
        if let Ok(mut locks) = process_locks().lock() {
            locks.remove(&key);
        }
    }

    fn file_type(mode: impl TryInto<u32>) -> u32 {
        mode.try_into().unwrap_or(u32::MAX) & FILE_TYPE_MASK
    }

    fn permissions(mode: impl TryInto<u32>) -> u32 {
        mode.try_into().unwrap_or(u32::MAX) & PERMISSION_MASK
    }

    fn state(message: impl Into<String>) -> ProvisioningLockError {
        ProvisioningLockError::State(message.into())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::provision_lock::{ProvisioningLock, ProvisioningLockError};
        use rustix::io::{fcntl_getfd, FdFlags};
        use std::fs::{self, File, OpenOptions};
        use std::io::{BufRead, BufReader, Read, Write};
        use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
        use std::os::unix::net::UnixListener;
        use std::process::{Command, Stdio};
        use std::sync::mpsc;
        use std::time::Duration;

        fn serialize() -> std::sync::MutexGuard<'static, ()> {
            crate::provision_lock::test_guard()
        }

        fn run_isolated(name: &str, scenario: fn()) {
            const SCENARIO_ENV: &str = "CHECKSY_TEST_PROVISION_LOCK_ISOLATED";
            if std::env::var(SCENARIO_ENV).as_deref() == Ok(name) {
                scenario();
                return;
            }

            let exact = format!("provision_lock::supported::tests::{name}");
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", &exact, "--nocapture", "--test-threads=1"])
                .env(SCENARIO_ENV, name)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let status = wait_bounded(&mut child);
            if !status.success() {
                let mut stderr = String::new();
                child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut stderr)
                    .unwrap();
                panic!("isolated lock scenario {name} failed: {status}: {stderr}");
            }
        }

        fn uid() -> u32 {
            geteuid().as_raw()
        }

        fn lock_directory(temporary: &tempfile::TempDir) -> PathBuf {
            temporary.path().join("checksy")
        }

        fn assert_state(error: ProvisioningLockError, fragment: &str) {
            match error {
                ProvisioningLockError::State(message) => assert!(
                    message.contains(fragment),
                    "state error {message:?} did not contain {fragment:?}"
                ),
                other => panic!("expected state failure, got {other:?}"),
            }
        }

        fn assert_any_state(error: ProvisioningLockError) {
            assert!(
                matches!(error, ProvisioningLockError::State(_)),
                "expected state failure, got {error:?}"
            );
        }

        fn creates_private_directory_and_persistent_cloexec_lock_scenario() {
            let _serial = serialize();
            let temporary = tempfile::tempdir().unwrap();
            let directory = lock_directory(&temporary);

            let lock = ProvisioningLock::acquire_at(&directory, uid()).unwrap();
            let directory_metadata = fs::metadata(&directory).unwrap();
            let path = directory.join("provision.lock");
            let metadata = fs::metadata(&path).unwrap();
            assert_eq!(directory_metadata.permissions().mode() & 0o7777, 0o700);
            assert_eq!(metadata.permissions().mode() & 0o7777, 0o600);
            assert_eq!(metadata.uid(), uid());
            assert_eq!(metadata.nlink(), 1);
            assert!(fcntl_getfd(lock.inner.descriptor.as_ref().unwrap())
                .unwrap()
                .contains(FdFlags::CLOEXEC));

            drop(lock);
            fs::write(&path, b"stale pid text must remain\n").unwrap();
            let lock = ProvisioningLock::acquire_at(&directory, uid()).unwrap();
            drop(lock);
            assert_eq!(fs::read(&path).unwrap(), b"stale pid text must remain\n");
        }

        fn same_inode_contends_and_independent_directories_do_not_scenario() {
            let _serial = serialize();
            let first = tempfile::tempdir().unwrap();
            let second = tempfile::tempdir().unwrap();
            let first_directory = lock_directory(&first);
            let second_directory = lock_directory(&second);

            let first_lock = ProvisioningLock::acquire_at(&first_directory, uid()).unwrap();
            assert_eq!(
                ProvisioningLock::acquire_at(&first_directory, uid()).unwrap_err(),
                ProvisioningLockError::Held
            );
            let second_lock = ProvisioningLock::acquire_at(&second_directory, uid()).unwrap();

            drop(first_lock);
            let reacquired = ProvisioningLock::acquire_at(&first_directory, uid()).unwrap();
            drop((reacquired, second_lock));
        }

        fn canonical_parent_aliases_identify_the_same_lock_scenario() {
            let _serial = serialize();
            let temporary = tempfile::tempdir().unwrap();
            let real_parent = temporary.path().join("real");
            fs::create_dir(&real_parent).unwrap();
            let alias_parent = temporary.path().join("alias");
            symlink(&real_parent, &alias_parent).unwrap();
            let real_directory = real_parent.join("checksy");
            let alias_directory = alias_parent.join("checksy");

            let lock = ProvisioningLock::acquire_at(&real_directory, uid()).unwrap();
            assert_eq!(
                ProvisioningLock::acquire_at(&alias_directory, uid()).unwrap_err(),
                ProvisioningLockError::Held
            );
            drop(lock);
            let reacquired = ProvisioningLock::acquire_at(&alias_directory, uid()).unwrap();
            drop(reacquired);
        }

        #[test]
        fn rejects_relative_directory_and_directory_integrity_failures() {
            let _serial = serialize();
            assert_state(
                ProvisioningLock::acquire_at(Path::new("relative"), uid()).unwrap_err(),
                "must be absolute",
            );

            let temporary = tempfile::tempdir().unwrap();
            let directory = lock_directory(&temporary);
            fs::create_dir(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
            assert_state(
                ProvisioningLock::acquire_at(&directory, uid()).unwrap_err(),
                "expected 0700",
            );

            fs::remove_dir(&directory).unwrap();
            let target = temporary.path().join("target");
            fs::create_dir(&target).unwrap();
            symlink(&target, &directory).unwrap();
            assert_state(
                ProvisioningLock::acquire_at(&directory, uid()).unwrap_err(),
                "cannot open provisioning directory",
            );
        }

        #[test]
        fn rejects_final_directory_substitution_before_locking() {
            let _serial = serialize();
            let temporary = tempfile::tempdir().unwrap();
            let directory = lock_directory(&temporary);
            fs::create_dir(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();

            let parent = open_directory(temporary.path(), "test lock parent").unwrap();
            let descriptor =
                open_or_create_directory(&parent, OsStr::new("checksy"), uid(), true).unwrap();
            let opened = OpenedLockDirectory {
                descriptor,
                parent,
                name: OsString::from("checksy"),
            };

            fs::rename(&directory, temporary.path().join("displaced-checksy")).unwrap();
            fs::create_dir(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();

            assert_state(
                acquire_in_directory(opened, uid()).unwrap_err(),
                "directory path changed",
            );
            assert!(!directory.join("provision.lock").exists());
        }

        #[test]
        fn rejects_lock_symlink_hardlink_mode_and_wrong_expected_owner() {
            let _serial = serialize();
            let temporary = tempfile::tempdir().unwrap();
            let directory = lock_directory(&temporary);
            let lock = ProvisioningLock::acquire_at(&directory, uid()).unwrap();
            drop(lock);
            let path = directory.join("provision.lock");

            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            assert_state(
                ProvisioningLock::acquire_at(&directory, uid()).unwrap_err(),
                "expected 0600",
            );

            fs::remove_file(&path).unwrap();
            let target = directory.join("target");
            File::create(&target).unwrap();
            symlink(&target, &path).unwrap();
            assert_state(
                ProvisioningLock::acquire_at(&directory, uid()).unwrap_err(),
                "cannot open provisioning lock",
            );

            fs::remove_file(&path).unwrap();
            fs::hard_link(&target, &path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            assert_state(
                ProvisioningLock::acquire_at(&directory, uid()).unwrap_err(),
                "links, expected 1",
            );

            fs::remove_file(&path).unwrap();
            fs::remove_file(&target).unwrap();
            let lock = ProvisioningLock::acquire_at(&directory, uid()).unwrap();
            drop(lock);
            let directory_descriptor = open_directory(&directory, "test lock directory").unwrap();
            let descriptor = openat(
                &directory_descriptor,
                "provision.lock",
                OFlags::RDWR | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .unwrap();
            assert_state(
                validate_lock_file(&descriptor, uid().wrapping_add(1)).unwrap_err(),
                "provisioning lock is owned by uid",
            );
            assert_state(
                ProvisioningLock::acquire_at(&directory, uid().wrapping_add(1)).unwrap_err(),
                "expected uid",
            );
        }

        #[test]
        fn rejects_nonregular_lock_entries() {
            let _serial = serialize();
            let temporary = tempfile::tempdir().unwrap();
            let directory = lock_directory(&temporary);
            fs::create_dir(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            let path = directory.join("provision.lock");

            fs::create_dir(&path).unwrap();
            assert_state(
                ProvisioningLock::acquire_at(&directory, uid()).unwrap_err(),
                "cannot open provisioning lock",
            );
            fs::remove_dir(&path).unwrap();

            let fifo = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
            let status = unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) };
            assert_eq!(status, 0);
            assert_state(
                ProvisioningLock::acquire_at(&directory, uid()).unwrap_err(),
                "not a regular file",
            );

            fs::remove_file(&path).unwrap();
            let _socket = UnixListener::bind(&path).unwrap();
            assert_any_state(ProvisioningLock::acquire_at(&directory, uid()).unwrap_err());

            let device = openat(
                rustix::fs::cwd(),
                "/dev/null",
                OFlags::RDWR | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .unwrap();
            assert_state(
                validate_lock_file(&device, uid()).unwrap_err(),
                "not a regular file",
            );
        }

        #[test]
        fn production_paths_match_the_per_effective_user_contract() {
            let _serial = serialize();
            #[cfg(target_os = "linux")]
            {
                let (anchor, suffix) = production_location(0).unwrap();
                assert_eq!(anchor, Path::new("/"));
                assert_eq!(suffix, ["var", "lib", "checksy"]);
                let (anchor, suffix) = non_root_location(PathBuf::from("/accounts/example"));
                assert_eq!(anchor, Path::new("/accounts/example"));
                assert_eq!(suffix, [".local", "state", "checksy"]);
                let home = account_home(uid()).unwrap();
                if uid() != 0 {
                    let (anchor, suffix) = production_location(uid()).unwrap();
                    assert_eq!(anchor, home);
                    assert_eq!(suffix, [".local", "state", "checksy"]);
                }
            }

            #[cfg(target_os = "macos")]
            {
                let (anchor, suffix) = production_location(0).unwrap();
                assert_eq!(anchor, Path::new("/"));
                assert_eq!(suffix, ["Library", "Application Support", "checksy"]);
                let (anchor, suffix) = non_root_location(PathBuf::from("/accounts/example"));
                assert_eq!(anchor, Path::new("/accounts/example"));
                assert_eq!(suffix, ["Library", "Application Support", "checksy"]);
                let home = account_home(uid()).unwrap();
                if uid() != 0 {
                    let (anchor, suffix) = production_location(uid()).unwrap();
                    assert_eq!(anchor, home);
                    assert_eq!(suffix, ["Library", "Application Support", "checksy"]);
                }
            }
        }

        #[test]
        fn cross_process_contention_and_release_are_immediate() {
            let _serial = serialize();
            let temporary = tempfile::tempdir().unwrap();
            let directory = lock_directory(&temporary);
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "provision_lock::supported::tests::provisioning_lock_holder_helper",
                    "--nocapture",
                ])
                .env("CHECKSY_TEST_PROVISION_LOCK_DIR", &directory)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();

            let _child_stdout = wait_for_ready(&mut child);

            assert_eq!(
                ProvisioningLock::acquire_at(&directory, uid()).unwrap_err(),
                ProvisioningLockError::Held
            );

            drop(child.stdin.take());
            let status = wait_bounded(&mut child);
            if !status.success() {
                let mut stderr = String::new();
                child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut stderr)
                    .unwrap();
                panic!("holder helper failed: {status}: {stderr}");
            }
            let reacquired = ProvisioningLock::acquire_at(&directory, uid()).unwrap();
            drop(reacquired);
        }

        #[test]
        fn process_death_releases_the_lock_immediately() {
            let _serial = serialize();
            let temporary = tempfile::tempdir().unwrap();
            let directory = lock_directory(&temporary);
            let mut child = spawn_holder(&directory);
            let _child_stdout = wait_for_ready(&mut child);
            assert_eq!(
                ProvisioningLock::acquire_at(&directory, uid()).unwrap_err(),
                ProvisioningLockError::Held
            );
            child.kill().unwrap();
            wait_bounded(&mut child);
            let reacquired = ProvisioningLock::acquire_at(&directory, uid()).unwrap();
            drop(reacquired);
        }

        fn exec_child_does_not_inherit_lock_ownership_scenario() {
            let _serial = serialize();
            let temporary = tempfile::tempdir().unwrap();
            let directory = lock_directory(&temporary);
            let lock = ProvisioningLock::acquire_at(&directory, uid()).unwrap();
            let mut child = Command::new("bash")
                .args(["-c", "printf 'ready\\n'; IFS= read -r _ || true"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let _child_stdout = wait_for_ready(&mut child);

            drop(lock);
            let reacquired = ProvisioningLock::acquire_at(&directory, uid()).unwrap();
            drop(reacquired);

            drop(child.stdin.take());
            let status = wait_bounded(&mut child);
            assert!(status.success());
        }

        fn spawn_holder(directory: &Path) -> std::process::Child {
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "provision_lock::supported::tests::provisioning_lock_holder_helper",
                    "--nocapture",
                ])
                .env("CHECKSY_TEST_PROVISION_LOCK_DIR", directory)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        }

        fn wait_for_ready(child: &mut std::process::Child) -> BufReader<std::process::ChildStdout> {
            let stdout = child.stdout.take().unwrap();
            let mut reader = BufReader::new(stdout);
            for _ in 0..32 {
                let descriptor = std::os::fd::AsRawFd::as_raw_fd(reader.get_ref());
                let mut poll = libc::pollfd {
                    fd: descriptor,
                    events: libc::POLLIN,
                    revents: 0,
                };
                assert_eq!(unsafe { libc::poll(&mut poll, 1, 5_000) }, 1);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "ready\n" {
                    return reader;
                }
                assert!(!line.is_empty(), "holder exited before readiness");
            }
            panic!("holder did not report readiness within bounded output");
        }

        fn wait_bounded(child: &mut std::process::Child) -> std::process::ExitStatus {
            let pid = child.id() as libc::pid_t;
            let (cancel, timeout) = mpsc::channel();
            let watchdog = std::thread::spawn(move || {
                if timeout.recv_timeout(Duration::from_secs(5)).is_err() {
                    // The helper is isolated and exists only for this test.
                    unsafe { libc::kill(pid, libc::SIGKILL) };
                }
            });
            let status = child.wait().unwrap();
            let _ = cancel.send(());
            watchdog.join().unwrap();
            status
        }

        #[test]
        #[ignore]
        fn provisioning_lock_holder_helper() {
            let Some(directory) = std::env::var_os("CHECKSY_TEST_PROVISION_LOCK_DIR") else {
                return;
            };
            let _lock = ProvisioningLock::acquire_at(Path::new(&directory), uid()).unwrap();
            println!("ready");
            std::io::stdout().flush().unwrap();
            let mut buffer = [0_u8; 1];
            while std::io::stdin().read(&mut buffer).unwrap() != 0 {}
        }

        fn stale_contents_are_ignored_and_preserved_scenario() {
            let _serial = serialize();
            let temporary = tempfile::tempdir().unwrap();
            let directory = lock_directory(&temporary);
            let lock = ProvisioningLock::acquire_at(&directory, uid()).unwrap();
            drop(lock);
            let path = directory.join("provision.lock");
            let fixture = fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("fixtures/provisioning-lock/stale-lock-contents.txt"),
            )
            .unwrap();
            let mut file = OpenOptions::new().write(true).open(&path).unwrap();
            file.write_all(&fixture).unwrap();
            drop(file);

            let lock = ProvisioningLock::acquire_at(&directory, uid()).unwrap();
            drop(lock);
            assert_eq!(fs::read(path).unwrap(), fixture);
        }

        macro_rules! isolated_lock_test {
            ($name:ident, $scenario:ident) => {
                #[test]
                fn $name() {
                    run_isolated(stringify!($name), $scenario);
                }
            };
        }

        isolated_lock_test!(
            creates_private_directory_and_persistent_cloexec_lock,
            creates_private_directory_and_persistent_cloexec_lock_scenario
        );
        isolated_lock_test!(
            same_inode_contends_and_independent_directories_do_not,
            same_inode_contends_and_independent_directories_do_not_scenario
        );
        isolated_lock_test!(
            canonical_parent_aliases_identify_the_same_lock,
            canonical_parent_aliases_identify_the_same_lock_scenario
        );
        isolated_lock_test!(
            exec_child_does_not_inherit_lock_ownership,
            exec_child_does_not_inherit_lock_ownership_scenario
        );
        isolated_lock_test!(
            stale_contents_are_ignored_and_preserved,
            stale_contents_are_ignored_and_preserved_scenario
        );
    }
}
