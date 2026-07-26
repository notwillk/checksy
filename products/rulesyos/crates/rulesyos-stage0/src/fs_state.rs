use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const ATOMIC_WRITE_ATTEMPTS: usize = 16;

#[derive(Debug)]
pub(crate) struct Stage0Lock {
    file: File,
}

impl Stage0Lock {
    pub(crate) fn acquire(path: &Path, expected_uid: u32) -> Result<Self, LockError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(path)
            .map_err(LockError::Io)?;
        let metadata = file.metadata().map_err(LockError::Io)?;
        if !metadata.file_type().is_file()
            || metadata.uid() != expected_uid
            || metadata.mode() & 0o7777 != 0o600
            || metadata.nlink() != 1
        {
            return Err(LockError::Io(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "{} must be a single-link UID {expected_uid}, mode 0600 regular file",
                    path.display()
                ),
            )));
        }
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result == 0 {
            return Ok(Self { file });
        }
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::WouldBlock {
            Err(LockError::Contended)
        } else {
            Err(LockError::Io(error))
        }
    }

    pub(crate) fn sync(&self) -> io::Result<()> {
        self.file.sync_all()
    }
}

#[derive(Debug)]
pub(crate) enum LockError {
    Contended,
    Io(io::Error),
}

#[derive(Clone, Debug)]
struct Mount {
    device: String,
    root: String,
    point: String,
    options: Vec<String>,
    filesystem: String,
}

pub(crate) fn validate_effective_user(require_root: bool, expected_uid: u32) -> Result<(), String> {
    let effective_uid = unsafe { libc::geteuid() };
    if require_root && effective_uid != 0 {
        return Err(format!(
            "stage zero must run as UID 0, found effective UID {effective_uid}"
        ));
    }
    if effective_uid != expected_uid {
        return Err(format!(
            "effective UID {effective_uid} does not match required UID {expected_uid}"
        ));
    }
    Ok(())
}

pub(crate) fn validate_private_dir(path: &Path, expected_uid: u32) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("inspect {}: {error}", path.display()))?;
    if !metadata.file_type().is_dir() {
        return Err(format!("{} is not a directory", path.display()));
    }
    if metadata.uid() != expected_uid {
        return Err(format!(
            "{} is owned by UID {}, expected {expected_uid}",
            path.display(),
            metadata.uid()
        ));
    }
    let mode = metadata.mode() & 0o7777;
    if mode != 0o700 {
        return Err(format!(
            "{} has mode {mode:04o}, expected 0700",
            path.display()
        ));
    }
    Ok(())
}

pub(crate) fn ensure_private_dir(path: &Path, expected_uid: u32) -> Result<(), String> {
    match fs::create_dir(path) {
        Ok(()) => fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("set mode on {}: {error}", path.display()))?,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(format!("create {}: {error}", path.display())),
    }
    validate_private_dir(path, expected_uid)
}

pub(crate) fn validate_mounts(
    mountinfo_path: &Path,
    root: &Path,
    state: &Path,
    rulesy_data: &Path,
) -> Result<(), String> {
    let contents = fs::read_to_string(mountinfo_path)
        .map_err(|error| format!("read {}: {error}", mountinfo_path.display()))?;
    let mounts = parse_mountinfo(&contents)?;
    let root_mount = exact_mount(&mounts, root)?;
    let state_mount = exact_mount(&mounts, state)?;
    let data_mount = exact_mount(&mounts, rulesy_data)?;

    require_option(root_mount, "ro")?;
    require_option(state_mount, "rw")?;
    require_option(data_mount, "rw")?;

    if root_mount.device == state_mount.device {
        return Err(format!(
            "{} must be a separate filesystem from {}",
            state.display(),
            root.display()
        ));
    }
    if data_mount.filesystem != "tmpfs" || data_mount.root == "/" {
        return Err(format!(
            "{} must be a writable bind mount backed by tmpfs",
            rulesy_data.display()
        ));
    }
    Ok(())
}

fn exact_mount<'a>(mounts: &'a [Mount], path: &Path) -> Result<&'a Mount, String> {
    let expected = path
        .to_str()
        .ok_or_else(|| format!("mount path {} is not UTF-8", path.display()))?;
    mounts
        .iter()
        .find(|mount| mount.point == expected)
        .ok_or_else(|| format!("{} is not a distinct mount point", path.display()))
}

fn require_option(mount: &Mount, option: &str) -> Result<(), String> {
    if mount.options.iter().any(|candidate| candidate == option) {
        Ok(())
    } else {
        Err(format!(
            "mount {} is missing required {option} option",
            mount.point
        ))
    }
}

fn parse_mountinfo(contents: &str) -> Result<Vec<Mount>, String> {
    contents
        .lines()
        .map(|line| {
            let (left, right) = line
                .split_once(" - ")
                .ok_or_else(|| format!("invalid mountinfo record: {line}"))?;
            let left: Vec<&str> = left.split_whitespace().collect();
            let right: Vec<&str> = right.split_whitespace().collect();
            if left.len() < 6 || right.is_empty() {
                return Err(format!("invalid mountinfo record: {line}"));
            }
            Ok(Mount {
                device: left[2].to_owned(),
                root: unescape_mount_field(left[3])?,
                point: unescape_mount_field(left[4])?,
                options: left[5].split(',').map(str::to_owned).collect(),
                filesystem: right[0].to_owned(),
            })
        })
        .collect()
}

fn unescape_mount_field(field: &str) -> Result<String, String> {
    let bytes = field.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            if index + 3 >= bytes.len() {
                return Err(format!("invalid mountinfo escape in {field}"));
            }
            let digits = &field[index + 1..index + 4];
            let value = u8::from_str_radix(digits, 8)
                .map_err(|_| format!("invalid mountinfo escape in {field}"))?;
            output.push(value);
            index += 4;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map_err(|_| format!("non-UTF-8 mountinfo field {field}"))
}

pub(crate) fn read_boot_id(path: &Path) -> Result<String, String> {
    let value = fs::read_to_string(path)
        .map_err(|error| format!("read boot ID from {}: {error}", path.display()))?;
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(format!("{} contains an invalid boot ID", path.display()));
    }
    Ok(value.to_owned())
}

pub(crate) fn read_verified_binary(path: &Path, expected_uid: u32) -> Result<Vec<u8>, String> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| format!("open {}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("inspect {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if metadata.uid() != expected_uid {
        return Err(format!(
            "{} is owned by UID {}, expected {expected_uid}",
            path.display(),
            metadata.uid()
        ));
    }
    let mode = metadata.mode() & 0o7777;
    if mode != 0o755 {
        return Err(format!(
            "{} has mode {mode:04o}, expected 0755",
            path.display()
        ));
    }
    if metadata.nlink() != 1 {
        return Err(format!(
            "{} has {} links, expected 1",
            path.display(),
            metadata.nlink()
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(bytes)
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    atomic_write_with_sequence(path, bytes, &TEMPORARY_SEQUENCE)
}

fn atomic_write_with_sequence(
    path: &Path,
    bytes: &[u8],
    sequence: &AtomicU64,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| format!("{} has no UTF-8 file name", path.display()))?;
    for _ in 0..ATOMIC_WRITE_ATTEMPTS {
        let current = sequence.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{file_name}.tmp.{}.{}",
            std::process::id(),
            current
        ));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("create {}: {error}", temporary.display()));
            }
        };
        let result = (|| {
            file.write_all(bytes)
                .map_err(|error| format!("write {}: {error}", temporary.display()))?;
            file.sync_all()
                .map_err(|error| format!("sync {}: {error}", temporary.display()))?;
            fs::rename(&temporary, path).map_err(|error| {
                format!(
                    "replace {} with {}: {error}",
                    path.display(),
                    temporary.display()
                )
            })?;
            sync_dir(parent)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return result;
    }
    Err(format!(
        "create a unique temporary file for {} after {ATOMIC_WRITE_ATTEMPTS} attempts",
        path.display()
    ))
}

pub(crate) fn sync_dir(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("sync directory {}: {error}", path.display()))
}

pub(crate) fn remove_file_and_sync(path: &Path) -> Result<(), String> {
    fs::remove_file(path).map_err(|error| format!("remove {}: {error}", path.display()))?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    sync_dir(parent)
}

pub(crate) fn writable(path: &Path) -> bool {
    let c_path = match std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) {
        Ok(path) => path,
        Err(_) => return false,
    };
    unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
}

pub(crate) fn private_regular_file(path: &Path, expected_uid: u32) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    metadata.file_type().is_file()
        && metadata.uid() == expected_uid
        && metadata.mode() & 0o7777 == 0o600
}

pub(crate) fn list_regular_files(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| format!("read {}: {error}", path.display()))? {
        let entry = entry.map_err(|error| format!("read {} entry: {error}", path.display()))?;
        let entry_path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("inspect {}: {error}", entry_path.display()))?;
        if !file_type.is_file() {
            return Err(format!(
                "refusing unexpected non-regular entry {}",
                entry_path.display()
            ));
        }
        files.push(entry_path);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::{
        atomic_write_with_sequence, parse_mountinfo, unescape_mount_field, validate_mounts,
        LockError, Stage0Lock,
    };
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn mountinfo_parser_decodes_paths_and_mount_properties() {
        let mounts = parse_mountinfo(
            "1 0 8:1 / / ro,relatime - ext4 /dev/root ro\n\
             2 1 8:2 / /state rw,nosuid - ext4 /dev/state rw\n\
             3 1 0:20 /source\\040dir /var/lib/rulesy rw - tmpfs tmpfs rw\n",
        )
        .unwrap();
        assert_eq!(mounts.len(), 3);
        assert_eq!(mounts[2].root, "/source dir");
        assert_eq!(mounts[2].point, "/var/lib/rulesy");
        assert_eq!(unescape_mount_field(r"a\134b").unwrap(), r"a\b");
    }

    #[test]
    fn singleton_lock_contends_releases_and_rejects_bad_modes() {
        let directory = std::env::temp_dir().join(format!(
            "rulesyos-lock-test-{}-{}",
            std::process::id(),
            super::super::tests::next_test_id()
        ));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let path = directory.join("stage0.lock");
        let uid = unsafe { libc::geteuid() };
        let first = Stage0Lock::acquire(&path, uid).unwrap();
        assert!(matches!(
            Stage0Lock::acquire(&path, uid),
            Err(LockError::Contended)
        ));
        drop(first);
        drop(Stage0Lock::acquire(&path, uid).unwrap());

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            Stage0Lock::acquire(&path, uid),
            Err(LockError::Io(_))
        ));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rulesy_data_must_be_a_tmpfs_backed_bind_mount() {
        let directory = std::env::temp_dir().join(format!(
            "rulesyos-mount-test-{}-{}",
            std::process::id(),
            super::super::tests::next_test_id()
        ));
        fs::create_dir(&directory).unwrap();
        let mountinfo = directory.join("mountinfo");
        fs::write(
            &mountinfo,
            "1 0 8:1 / / ro,relatime - ext2 /dev/root ro\n\
             2 1 8:2 / /state rw,nosuid - ext4 /dev/state rw\n\
             3 1 8:2 /rulesy /var/lib/rulesy rw - ext4 /dev/state rw\n",
        )
        .unwrap();
        let error = validate_mounts(
            &mountinfo,
            Path::new("/"),
            Path::new("/state"),
            Path::new("/var/lib/rulesy"),
        )
        .unwrap_err();
        assert!(error.contains("bind mount backed by tmpfs"), "{error}");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn atomic_write_skips_a_stale_temporary_file() {
        let directory = std::env::temp_dir().join(format!(
            "rulesyos-atomic-test-{}-{}",
            std::process::id(),
            super::super::tests::next_test_id()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("latest.json");
        let sequence = AtomicU64::new(7);
        let stale = directory.join(format!(".latest.json.tmp.{}.7", std::process::id()));
        fs::write(&stale, b"stale").unwrap();

        atomic_write_with_sequence(&path, b"current", &sequence).unwrap();

        assert_eq!(fs::read(path).unwrap(), b"current");
        assert_eq!(fs::read(stale).unwrap(), b"stale");
        fs::remove_dir_all(directory).unwrap();
    }
}
