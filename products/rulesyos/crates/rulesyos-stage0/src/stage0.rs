use crate::config::RuntimeConfig;
use crate::fs_state::{
    atomic_write, ensure_private_dir, read_boot_id, read_verified_binary, validate_effective_user,
    validate_mounts, validate_private_dir, writable, LockError, Stage0Lock,
};
use crate::hash::{hex, sha256};
use crate::process::{self, ProcessResult, ProcessSpec, Termination};
use crate::status::{self, Outcome, Status};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::time::{Duration, Instant};

const VERSION_TIMEOUT: Duration = Duration::from_secs(10);
const VERSION_OUTPUT_LIMIT: usize = 4096;

pub(crate) fn execute(config: &RuntimeConfig, output: &mut impl Write) -> i32 {
    let started = Instant::now();
    let config_digest = format!("sha256:{}", hex(&sha256(&config.baked_config)));
    let mut boot_id = String::new();
    let mut rulesy_digest = String::new();

    if let Err(error) = validate_effective_user(config.require_root, config.expected_uid) {
        eprintln!("rulesyos-stage0: {error}");
        return 1;
    }
    if let Err(error) = validate_state_foundation(config) {
        eprintln!("rulesyos-stage0: {error}");
        return 1;
    }
    if let Err(error) = prepare_state_directories(config) {
        eprintln!("rulesyos-stage0: {error}");
        return 1;
    }
    let lock = match Stage0Lock::acquire(&config.lock, config.expected_uid) {
        Ok(lock) => lock,
        Err(LockError::Contended) => {
            eprintln!("rulesyos-stage0: another stage-zero transition holds the singleton lock");
            return 1;
        }
        Err(LockError::Io(error)) => {
            eprintln!(
                "rulesyos-stage0: acquire singleton lock {}: {error}",
                config.lock.display()
            );
            return 1;
        }
    };
    if let Err(error) = lock.sync() {
        eprintln!(
            "rulesyos-stage0: sync singleton lock {}: {error}",
            config.lock.display()
        );
        return 1;
    }

    let preflight = (|| {
        validate_mounts(
            &config.mountinfo,
            &config.root,
            &config.state,
            &config.rulesy_data,
        )?;
        validate_private_dir(&config.rulesy_data, config.expected_uid)?;
        if !writable(&config.state) {
            return Err(format!("{} is not writable", config.state.display()));
        }
        if !writable(&config.rulesy_data) {
            return Err(format!("{} is not writable", config.rulesy_data.display()));
        }
        boot_id = read_boot_id(&config.boot_id)?;
        materialize_baked_config(config)?;
        let binary = read_verified_binary(&config.rulesy, config.expected_uid)?;
        let actual_digest = hex(&sha256(&binary));
        rulesy_digest = format!("sha256:{actual_digest}");
        if actual_digest != config.rulesy_sha256 {
            return Err(format!(
                "{} SHA-256 {actual_digest} does not match pinned {}",
                config.rulesy.display(),
                config.rulesy_sha256
            ));
        }
        validate_rulesy_version(config)
    })();

    if let Err(error) = preflight {
        let status = base_status(
            config,
            &boot_id,
            &config_digest,
            &rulesy_digest,
            Outcome::FirmwareDegraded,
            false,
            None,
            None,
            started.elapsed(),
        );
        let stderr = format!("{error}\n");
        if let Err(persist_error) =
            persist_and_report(config, &status, &[], stderr.as_bytes(), output)
        {
            eprintln!(
                "rulesyos-stage0: {error}; additionally failed to persist status: {persist_error}"
            );
        } else {
            eprintln!("rulesyos-stage0: {error}");
        }
        return 1;
    }

    let result = invoke_rulesy(config);
    match result {
        Ok(result) => {
            let (outcome, healthy, rulesy_exit, rulesy_signal) = classify(&result.termination);
            let status = base_status(
                config,
                &boot_id,
                &config_digest,
                &rulesy_digest,
                outcome,
                healthy,
                rulesy_exit,
                rulesy_signal,
                result.duration,
            );
            match persist_and_report(config, &status, &result.stdout, &result.stderr, output) {
                Ok(()) => 0,
                Err(error) => {
                    eprintln!("rulesyos-stage0: {error}");
                    1
                }
            }
        }
        Err(error) => {
            let status = base_status(
                config,
                &boot_id,
                &config_digest,
                &rulesy_digest,
                Outcome::FirmwareDegraded,
                false,
                None,
                None,
                started.elapsed(),
            );
            let stderr = format!("{error}\n");
            match persist_and_report(config, &status, &[], stderr.as_bytes(), output) {
                Ok(()) => 1,
                Err(persist_error) => {
                    eprintln!(
                        "rulesyos-stage0: {error}; additionally failed to persist status: {persist_error}"
                    );
                    1
                }
            }
        }
    }
}

fn validate_state_foundation(config: &RuntimeConfig) -> Result<(), String> {
    validate_private_dir(&config.state, config.expected_uid)?;
    let contents = fs::read_to_string(&config.mountinfo)
        .map_err(|error| format!("read {}: {error}", config.mountinfo.display()))?;
    let root = config.root.display().to_string();
    let state = config.state.display().to_string();
    let root_record = contents
        .lines()
        .find(|line| mount_point(line) == Some(root.as_str()))
        .ok_or_else(|| format!("{} is not a mount point", config.root.display()))?;
    let state_record = contents
        .lines()
        .find(|line| mount_point(line) == Some(state.as_str()))
        .ok_or_else(|| format!("{} is not a mount point", config.state.display()))?;
    let root_device = root_record
        .split_whitespace()
        .nth(2)
        .ok_or_else(|| "root mount record has no device".to_owned())?;
    let state_fields: Vec<_> = state_record.split_whitespace().collect();
    if state_fields.get(2).copied() == Some(root_device) {
        return Err(format!(
            "{} must be a separate filesystem from {}",
            config.state.display(),
            config.root.display()
        ));
    }
    if !state_fields
        .get(5)
        .is_some_and(|options| options.split(',').any(|option| option == "rw"))
    {
        return Err(format!(
            "{} is not mounted writable",
            config.state.display()
        ));
    }
    Ok(())
}

fn mount_point(record: &str) -> Option<&str> {
    record.split_whitespace().nth(4)
}

fn prepare_state_directories(config: &RuntimeConfig) -> Result<(), String> {
    ensure_private_dir(&config.rulesyos_dir(), config.expected_uid)?;
    ensure_private_dir(&config.status_dir(), config.expected_uid)?;
    ensure_private_dir(&config.logs, config.expected_uid)?;
    ensure_private_dir(&config.lock_dir(), config.expected_uid)
}

fn materialize_baked_config(config: &RuntimeConfig) -> Result<(), String> {
    let config_dir = config.config_dir();
    let config_root = config_dir
        .parent()
        .ok_or_else(|| format!("{} has no RulesyOS run parent", config.config.display()))?;
    let rulesyos_run = config_root
        .parent()
        .ok_or_else(|| format!("{} has no runtime root", config.config.display()))?;
    let run_root = rulesyos_run
        .parent()
        .ok_or_else(|| format!("{} has no runtime root", rulesyos_run.display()))?;
    if !run_root.exists() {
        return Err(format!(
            "runtime directory {} is absent",
            run_root.display()
        ));
    }
    ensure_private_dir(rulesyos_run, config.expected_uid)?;
    ensure_private_dir(config_root, config.expected_uid)?;
    ensure_private_dir(&config_dir, config.expected_uid)?;
    atomic_write(&config.config, &config.baked_config)
}

fn validate_rulesy_version(config: &RuntimeConfig) -> Result<(), String> {
    let spec = ProcessSpec::new(
        &config.rulesy,
        [OsString::from("--version")],
        &config.root,
        VERSION_TIMEOUT,
        config.term_grace,
        VERSION_OUTPUT_LIMIT,
    );
    let result = process::run(&spec)?;
    let expected = format!("rulesy {}\n", config.rulesy_version);
    if result.termination != Termination::Exited(0)
        || result.stdout != expected.as_bytes()
        || !result.stderr.is_empty()
    {
        return Err(format!(
            "{} --version did not return exactly {:?}",
            config.rulesy.display(),
            expected.trim_end()
        ));
    }
    Ok(())
}

fn invoke_rulesy(config: &RuntimeConfig) -> Result<ProcessResult, String> {
    let spec = ProcessSpec::new(
        &config.rulesy,
        config.rulesy_args().into_iter().map(OsString::from),
        &config.root,
        config.timeout,
        config.term_grace,
        config.output_limit,
    );
    process::run(&spec)
}

fn classify(termination: &Termination) -> (Outcome, bool, Option<i32>, Option<i32>) {
    match termination {
        Termination::Exited(0) => (Outcome::Converged, true, Some(0), None),
        Termination::Exited(3) => (Outcome::ComplianceFailed, true, Some(3), None),
        Termination::Exited(4) => (Outcome::LockContended, true, Some(4), None),
        Termination::Exited(code) => (Outcome::OperationalFailed, true, Some(*code), None),
        Termination::Signaled(signal) | Termination::Interrupted(signal) => {
            (Outcome::Interrupted, true, None, Some(*signal))
        }
        Termination::TimedOut => (Outcome::Interrupted, true, None, None),
    }
}

#[allow(clippy::too_many_arguments)]
fn base_status(
    config: &RuntimeConfig,
    boot_id: &str,
    config_digest: &str,
    rulesy_digest: &str,
    outcome: Outcome,
    firmware_healthy: bool,
    rulesy_exit: Option<i32>,
    rulesy_signal: Option<i32>,
    duration: Duration,
) -> Status {
    Status {
        schema_version: 1,
        boot_id: boot_id.to_owned(),
        rulesyos_version: config.rulesyos_version.clone(),
        firmware_slot: None,
        firmware_healthy,
        source_kind: "baked".to_owned(),
        config_digest: config_digest.to_owned(),
        candidate_digest: None,
        rulesy_version: config.rulesy_version.clone(),
        rulesy_digest: rulesy_digest.to_owned(),
        rulesy_exit,
        rulesy_signal,
        outcome,
        duration_ms: duration.as_millis().min(u128::from(u64::MAX)) as u64,
    }
}

fn persist_and_report(
    config: &RuntimeConfig,
    stage_status: &Status,
    stdout: &[u8],
    stderr: &[u8],
    output: &mut impl Write,
) -> Result<(), String> {
    let json = status::persist(
        &config.status,
        &config.logs,
        stage_status,
        stdout,
        stderr,
        config.log_limit,
        config.expected_uid,
    )?;
    let console_result = output
        .write_all(stdout)
        .and_then(|()| {
            if stdout.is_empty() || stdout.ends_with(b"\n") {
                Ok(())
            } else {
                output.write_all(b"\n")
            }
        })
        .and_then(|()| output.write_all(stderr))
        .and_then(|()| {
            if stderr.is_empty() || stderr.ends_with(b"\n") {
                Ok(())
            } else {
                output.write_all(b"\n")
            }
        })
        .and_then(|()| output.write_all(b"RULESYOS_STATUS "))
        .and_then(|()| output.write_all(&json))
        .and_then(|()| output.write_all(b"\n"))
        .and_then(|()| output.flush());
    if let Err(error) = console_result {
        eprintln!("rulesyos-stage0: write status to console: {error}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{classify, execute, Outcome};
    use crate::config::{RuntimeConfig, BAKED_CONFIG};
    use crate::hash::{hex, sha256};
    use crate::process::Termination;
    use crate::status::Status;
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    struct Fixture {
        root: PathBuf,
        exit_code: PathBuf,
        break_status: PathBuf,
        remove_after_version: PathBuf,
        config: RuntimeConfig,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "rulesyos-stage0-test-{}-{}",
                std::process::id(),
                super::super::tests::next_test_id()
            ));
            private_dir(&root);
            let state = root.join("state");
            let data = root.join("rulesy-data");
            let run = root.join("run");
            private_dir(&state);
            private_dir(&data);
            private_dir(&run);
            let boot_id = root.join("boot-id");
            fs::write(&boot_id, "11111111-2222-3333-4444-555555555555\n").unwrap();
            let mountinfo = root.join("mountinfo");
            fs::write(
                &mountinfo,
                format!(
                    "1 0 8:1 / / ro,relatime - ext4 /dev/root ro\n\
                     2 1 8:2 / {} rw,nosuid - ext4 /dev/state rw\n\
                     3 1 0:20 /rulesy-lock {} rw,nosuid - tmpfs tmpfs rw\n",
                    state.display(),
                    data.display()
                ),
            )
            .unwrap();
            let exit_code = root.join("exit-code");
            let break_status = root.join("break-status");
            let remove_after_version = root.join("remove-after-version");
            fs::write(&exit_code, "0\n").unwrap();
            let rulesy = root.join("rulesy");
            let status = state.join("rulesyos/status/latest.json");
            let script = format!(
                "#!/usr/bin/python3\n\
                 import json, os, pathlib, sys\n\
                 if sys.argv[1:] == ['--version']:\n\
                 \x20   print('rulesy 0.8.2')\n\
                 \x20   if pathlib.Path('{}').exists():\n\
                 \x20       pathlib.Path(sys.argv[0]).unlink()\n\
                 \x20   raise SystemExit(0)\n\
                 print(json.dumps({{'args': sys.argv[1:], 'cwd': os.getcwd(), 'env': dict(os.environ), 'stdin_eof': os.read(0, 1) == b''}}, sort_keys=True, separators=(',', ':')))\n\
                 print('RULESYOS_BAKED_FIX_APPLIED')\n\
                 print('rulesy stderr', file=sys.stderr)\n\
                 if pathlib.Path('{}').exists():\n\
                 \x20   os.chmod('{}', 0o500)\n\
                 raise SystemExit(int(pathlib.Path('{}').read_text().strip()))\n",
                remove_after_version.display(),
                break_status.display(),
                status.parent().unwrap().display(),
                exit_code.display()
            );
            fs::write(&rulesy, script).unwrap();
            fs::set_permissions(&rulesy, fs::Permissions::from_mode(0o755)).unwrap();
            let digest = hex(&sha256(&fs::read(&rulesy).unwrap()));
            let uid = unsafe { libc::geteuid() };
            let config = RuntimeConfig {
                root: PathBuf::from("/"),
                state: state.clone(),
                rulesy_data: data,
                config: run.join("rulesyos/config/current/rulesy.yaml"),
                rulesy,
                boot_id,
                mountinfo,
                status,
                logs: state.join("rulesyos/logs"),
                lock: state.join("rulesyos/locks/stage0.lock"),
                baked_config: BAKED_CONFIG.to_vec(),
                rulesyos_version: "0.1.0".to_owned(),
                rulesy_version: "0.8.2".to_owned(),
                rulesy_sha256: digest,
                expected_uid: uid,
                require_root: false,
                timeout: Duration::from_secs(2),
                term_grace: Duration::from_millis(50),
                output_limit: 256 * 1024,
                log_limit: 8,
            };
            Self {
                root,
                exit_code,
                break_status,
                remove_after_version,
                config,
            }
        }

        fn status(&self) -> Status {
            serde_json::from_slice(&fs::read(&self.config.status).unwrap()).unwrap()
        }

        fn log(&self) -> Vec<u8> {
            fs::read(
                self.config
                    .logs
                    .join("11111111-2222-3333-4444-555555555555.log"),
            )
            .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Some(status_dir) = self.config.status.parent() {
                let _ = fs::set_permissions(status_dir, fs::Permissions::from_mode(0o700));
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn private_dir(path: &Path) {
        fs::create_dir(path).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn rulesy_exit_classes_map_to_stable_outcomes() {
        assert_eq!(
            classify(&Termination::Exited(0)),
            (Outcome::Converged, true, Some(0), None)
        );
        assert_eq!(
            classify(&Termination::Exited(1)),
            (Outcome::OperationalFailed, true, Some(1), None)
        );
        assert_eq!(
            classify(&Termination::Exited(2)),
            (Outcome::OperationalFailed, true, Some(2), None)
        );
        assert_eq!(
            classify(&Termination::Exited(3)),
            (Outcome::ComplianceFailed, true, Some(3), None)
        );
        assert_eq!(
            classify(&Termination::Exited(4)),
            (Outcome::LockContended, true, Some(4), None)
        );
        assert_eq!(
            classify(&Termination::Exited(99)),
            (Outcome::OperationalFailed, true, Some(99), None)
        );
        assert_eq!(
            classify(&Termination::TimedOut),
            (Outcome::Interrupted, true, None, None)
        );
        assert_eq!(
            classify(&Termination::Signaled(libc::SIGKILL)),
            (Outcome::Interrupted, true, None, Some(libc::SIGKILL))
        );
        assert_eq!(
            classify(&Termination::Interrupted(libc::SIGTERM)),
            (Outcome::Interrupted, true, None, Some(libc::SIGTERM))
        );
    }

    #[test]
    fn complete_run_materializes_baked_config_and_persists_exact_contract() {
        let fixture = Fixture::new();
        let mut console = Vec::new();
        assert_eq!(execute(&fixture.config, &mut console), 0);

        assert_eq!(
            fs::read(&fixture.config.config).unwrap(),
            fixture.config.baked_config
        );
        let config_metadata = fs::metadata(&fixture.config.config).unwrap();
        assert_eq!(config_metadata.mode() & 0o7777, 0o600);
        assert_eq!(config_metadata.uid(), fixture.config.expected_uid);

        let status = fixture.status();
        assert_eq!(status.schema_version, 1);
        assert_eq!(status.boot_id, "11111111-2222-3333-4444-555555555555");
        assert_eq!(status.rulesyos_version, "0.1.0");
        assert_eq!(status.firmware_slot, None);
        assert!(status.firmware_healthy);
        assert_eq!(status.source_kind, "baked");
        assert_eq!(
            status.config_digest,
            format!("sha256:{}", hex(&sha256(BAKED_CONFIG)))
        );
        assert_eq!(status.candidate_digest, None);
        assert_eq!(status.rulesy_version, "0.8.2");
        assert_eq!(
            status.rulesy_digest,
            format!("sha256:{}", fixture.config.rulesy_sha256)
        );
        assert_eq!(status.rulesy_exit, Some(0));
        assert_eq!(status.rulesy_signal, None);
        assert_eq!(status.outcome, Outcome::Converged);

        let expected_status = format!(
            "RULESYOS_STATUS {}\n",
            String::from_utf8(status.compact_json().unwrap()).unwrap()
        );
        let console = String::from_utf8(console).unwrap();
        assert!(console.contains("RULESYOS_BAKED_FIX_APPLIED\n"));
        assert!(console.contains("rulesy stderr\n"));
        assert!(console.ends_with(&expected_status));

        let log = String::from_utf8(fixture.log()).unwrap();
        let expected_args = format!(
            r#""args":["--config={}","check","--fix","--non-interactive"]"#,
            fixture.config.config.display()
        );
        assert!(log.contains(&expected_args), "{log}");
        assert!(log.contains(r#""cwd":"/""#), "{log}");
        assert!(
            log.contains(
                r#""env":{"HOME":"/root","LANG":"C","LC_ALL":"C","PATH":"/usr/bin:/bin:/usr/sbin:/sbin"}"#
            ),
            "{log}"
        );
        assert!(log.contains(r#""stdin_eof":true"#), "{log}");
        assert!(log.contains("RULESYOS_BAKED_FIX_APPLIED"), "{log}");
        assert!(log.contains("rulesy stderr"), "{log}");
    }

    #[test]
    fn mismatched_binary_digest_is_firmware_degraded_and_fails_stage_zero() {
        let mut fixture = Fixture::new();
        fixture.config.rulesy_sha256 = "0".repeat(64);
        let mut console = Vec::new();
        assert_eq!(execute(&fixture.config, &mut console), 1);
        let status = fixture.status();
        assert_eq!(status.outcome, Outcome::FirmwareDegraded);
        assert!(!status.firmware_healthy);
        assert_eq!(status.rulesy_exit, None);
        assert!(status.rulesy_digest.starts_with("sha256:"));
        assert_ne!(status.rulesy_digest, "sha256:");
        assert!(String::from_utf8(console)
            .unwrap()
            .contains("RULESYOS_STATUS {"));
    }

    #[test]
    fn mismatched_rulesy_version_is_firmware_degraded_and_fails_stage_zero() {
        let mut fixture = Fixture::new();
        fixture.config.rulesy_version = "0.8.3".to_owned();
        let mut console = Vec::new();
        assert_eq!(execute(&fixture.config, &mut console), 1);
        let status = fixture.status();
        assert_eq!(status.outcome, Outcome::FirmwareDegraded);
        assert!(!status.firmware_healthy);
        assert_eq!(status.rulesy_exit, None);
        assert!(console.ends_with(b"\n"));
    }

    #[test]
    fn unexpected_rulesy_mode_is_firmware_degraded_and_fails_stage_zero() {
        let fixture = Fixture::new();
        fs::set_permissions(&fixture.config.rulesy, fs::Permissions::from_mode(0o700)).unwrap();
        let mut console = Vec::new();
        assert_eq!(execute(&fixture.config, &mut console), 1);
        let status = fixture.status();
        assert_eq!(status.outcome, Outcome::FirmwareDegraded);
        assert!(!status.firmware_healthy);
        assert_eq!(status.rulesy_exit, None);
    }

    #[test]
    fn child_failure_is_durable_and_does_not_fail_stage_zero() {
        let fixture = Fixture::new();
        fs::write(&fixture.exit_code, "3\n").unwrap();
        let mut console = Vec::new();
        assert_eq!(execute(&fixture.config, &mut console), 0);
        let status = fixture.status();
        assert_eq!(status.outcome, Outcome::ComplianceFailed);
        assert_eq!(status.rulesy_exit, Some(3));
        assert!(status.firmware_healthy);
    }

    #[test]
    fn status_durability_failure_fails_stage_zero() {
        let fixture = Fixture::new();
        fs::write(&fixture.break_status, b"break\n").unwrap();
        let mut console = Vec::new();
        assert_eq!(execute(&fixture.config, &mut console), 1);
        assert!(console.is_empty());
    }

    #[test]
    fn rulesy_invocation_failure_is_firmware_degraded_and_fails_stage_zero() {
        let fixture = Fixture::new();
        fs::write(&fixture.remove_after_version, b"remove\n").unwrap();
        let mut console = Vec::new();
        assert_eq!(execute(&fixture.config, &mut console), 1);
        let status = fixture.status();
        assert_eq!(status.outcome, Outcome::FirmwareDegraded);
        assert!(!status.firmware_healthy);
        assert_eq!(status.rulesy_exit, None);
        assert_eq!(status.rulesy_signal, None);
        assert!(String::from_utf8(console)
            .unwrap()
            .contains("RULESYOS_STATUS {"));
    }

    struct FailingWriter;

    impl std::io::Write for FailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "test console failure",
            ))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn handled_outcome_succeeds_after_durable_status_when_console_fails() {
        let fixture = Fixture::new();
        assert_eq!(execute(&fixture.config, &mut FailingWriter), 0);
        assert_eq!(fixture.status().outcome, Outcome::Converged);
    }

    #[test]
    fn embedded_configuration_has_one_fix_and_one_loopback_check() {
        let config = std::str::from_utf8(BAKED_CONFIG).unwrap();
        assert_eq!(config.matches("    fix: |").count(), 1);
        assert!(config.contains("/state/rulesyos/baked-config-applied"));
        assert!(config.contains("RULESYOS_BAKED_FIX_APPLIED"));
        assert!(config.contains("set -- /sys/class/net/*"));
        assert!(config.contains("test \"$1\" = /sys/class/net/lo"));
    }
}
