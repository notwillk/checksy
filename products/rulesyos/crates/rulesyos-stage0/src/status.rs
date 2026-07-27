use crate::fs_state::{
    atomic_write, list_regular_files, private_regular_file, remove_file_and_sync, sync_dir,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Outcome {
    Converged,
    ComplianceFailed,
    OperationalFailed,
    LockContended,
    Interrupted,
    FirmwareDegraded,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub(crate) schema_version: u32,
    pub(crate) boot_id: String,
    pub(crate) rulesyos_version: String,
    pub(crate) firmware_slot: Option<String>,
    pub(crate) firmware_healthy: bool,
    pub(crate) source_kind: String,
    pub(crate) config_digest: String,
    pub(crate) candidate_digest: Option<String>,
    pub(crate) rulesy_version: String,
    pub(crate) rulesy_digest: String,
    pub(crate) rulesy_exit: Option<i32>,
    pub(crate) rulesy_signal: Option<i32>,
    pub(crate) outcome: Outcome,
    pub(crate) duration_ms: u64,
}

impl Status {
    pub(crate) fn compact_json(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|error| format!("serialize status: {error}"))
    }
}

pub(crate) fn persist(
    status_path: &Path,
    logs_dir: &Path,
    status: &Status,
    stdout: &[u8],
    stderr: &[u8],
    log_limit: usize,
    expected_uid: u32,
) -> Result<Vec<u8>, String> {
    let json = status.compact_json()?;
    validate_log_files(logs_dir, expected_uid)?;
    write_log(logs_dir, status, stdout, stderr)?;
    rotate_logs(logs_dir, log_limit, expected_uid)?;
    let mut document = json.clone();
    document.push(b'\n');
    atomic_write(status_path, &document)?;
    Ok(json)
}

fn write_log(logs_dir: &Path, status: &Status, stdout: &[u8], stderr: &[u8]) -> Result<(), String> {
    let name = if status.boot_id.is_empty() {
        format!("unavailable-{}.log", std::process::id())
    } else {
        format!("{}.log", status.boot_id)
    };
    let path = logs_dir.join(name);
    let mut bytes = Vec::with_capacity(stdout.len() + stderr.len() + 256);
    bytes.extend_from_slice(b"RULESYOS_STATUS ");
    bytes.extend_from_slice(&status.compact_json()?);
    bytes.extend_from_slice(b"\n--- rulesy stdout ---\n");
    bytes.extend_from_slice(stdout);
    if !stdout.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    bytes.extend_from_slice(b"--- rulesy stderr ---\n");
    bytes.extend_from_slice(stderr);
    if !stderr.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    atomic_write(&path, &bytes)
}

fn rotate_logs(directory: &Path, limit: usize, expected_uid: u32) -> Result<(), String> {
    let mut files = validate_log_files(directory, expected_uid)?;
    files.sort_by_key(|path| (modified_key(path), path.clone()));
    let remove_count = files.len().saturating_sub(limit);
    for path in files.into_iter().take(remove_count) {
        remove_file_and_sync(&path)?;
    }
    if remove_count == 0 {
        sync_dir(directory)?;
    }
    Ok(())
}

fn validate_log_files(directory: &Path, expected_uid: u32) -> Result<Vec<PathBuf>, String> {
    let files = list_regular_files(directory)?;
    for path in &files {
        if !private_regular_file(path, expected_uid) {
            return Err(format!(
                "refusing unexpected non-private log {}",
                path.display()
            ));
        }
    }
    Ok(files)
}

fn modified_key(path: &PathBuf) -> u128 {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{persist, Outcome, Status};
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::PathBuf;

    fn status(boot_id: &str) -> Status {
        Status {
            schema_version: 1,
            boot_id: boot_id.to_owned(),
            rulesyos_version: "0.1.0".to_owned(),
            firmware_slot: None,
            firmware_healthy: true,
            source_kind: "baked".to_owned(),
            config_digest: "sha256:config".to_owned(),
            candidate_digest: None,
            rulesy_version: "0.8.3".to_owned(),
            rulesy_digest: "sha256:rulesy".to_owned(),
            rulesy_exit: Some(0),
            rulesy_signal: None,
            outcome: Outcome::Converged,
            duration_ms: 1,
        }
    }

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "rulesyos-status-test-{}-{}",
            std::process::id(),
            super::super::tests::next_test_id()
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[test]
    fn status_is_atomic_compact_and_logs_are_private_and_bounded() {
        let root = temp_dir();
        let logs = root.join("logs");
        fs::create_dir(&logs).unwrap();
        fs::set_permissions(&logs, fs::Permissions::from_mode(0o700)).unwrap();
        let latest = root.join("latest.json");
        let uid = unsafe { libc::geteuid() };
        for index in 0..10 {
            let current = status(&format!("boot-{index:02}"));
            let json = persist(&latest, &logs, &current, b"stdout", b"stderr", 8, uid).unwrap();
            assert!(!json.contains(&b'\n'));
        }
        let files: Vec<_> = fs::read_dir(&logs).unwrap().collect();
        assert_eq!(files.len(), 8);
        assert!(!logs.join("boot-00.log").exists());
        assert!(!logs.join("boot-01.log").exists());
        let metadata = fs::metadata(&latest).unwrap();
        assert_eq!(metadata.mode() & 0o7777, 0o600);
        assert_eq!(metadata.uid(), uid);
        let parsed: Status = serde_json::from_slice(&fs::read(&latest).unwrap()).unwrap();
        assert_eq!(parsed.boot_id, "boot-09");
    }

    #[test]
    fn status_rejects_non_regular_log_entries_before_writing() {
        let root = temp_dir();
        let logs = root.join("logs");
        fs::create_dir(&logs).unwrap();
        fs::set_permissions(&logs, fs::Permissions::from_mode(0o700)).unwrap();
        let unexpected = logs.join("unexpected");
        fs::create_dir(&unexpected).unwrap();
        let latest = root.join("latest.json");
        let uid = unsafe { libc::geteuid() };

        let error = persist(
            &latest,
            &logs,
            &status("boot"),
            b"stdout",
            b"stderr",
            8,
            uid,
        )
        .unwrap_err();

        assert!(error.contains("unexpected non-regular entry"), "{error}");
        assert!(error.contains(&unexpected.display().to_string()), "{error}");
        assert!(!logs.join("boot.log").exists());
        assert!(!latest.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
