use std::path::PathBuf;
use std::time::Duration;

pub(crate) const RULESYOS_VERSION: &str = "0.1.0";
pub(crate) const RULESY_VERSION: &str = "0.8.2";
pub(crate) const RULESY_SHA256: &str =
    "4f1ca0cd30e85d450247973d672460ed2141496bab4b656c0787d0f65e391f39";
pub(crate) const BAKED_CONFIG: &[u8] = include_bytes!("../assets/baked-rulesy.yaml");

#[derive(Clone, Debug)]
pub(crate) struct RuntimeConfig {
    pub(crate) root: PathBuf,
    pub(crate) state: PathBuf,
    pub(crate) rulesy_data: PathBuf,
    pub(crate) config: PathBuf,
    pub(crate) rulesy: PathBuf,
    pub(crate) boot_id: PathBuf,
    pub(crate) mountinfo: PathBuf,
    pub(crate) status: PathBuf,
    pub(crate) logs: PathBuf,
    pub(crate) lock: PathBuf,
    pub(crate) baked_config: Vec<u8>,
    pub(crate) rulesyos_version: String,
    pub(crate) rulesy_version: String,
    pub(crate) rulesy_sha256: String,
    pub(crate) expected_uid: u32,
    pub(crate) require_root: bool,
    pub(crate) timeout: Duration,
    pub(crate) term_grace: Duration,
    pub(crate) output_limit: usize,
    pub(crate) log_limit: usize,
}

impl RuntimeConfig {
    pub(crate) fn production() -> Self {
        Self {
            root: PathBuf::from("/"),
            state: PathBuf::from("/state"),
            rulesy_data: PathBuf::from("/var/lib/rulesy"),
            config: PathBuf::from("/run/rulesyos/config/current/rulesy.yaml"),
            rulesy: PathBuf::from("/usr/bin/rulesy"),
            boot_id: PathBuf::from("/proc/sys/kernel/random/boot_id"),
            mountinfo: PathBuf::from("/proc/self/mountinfo"),
            status: PathBuf::from("/state/rulesyos/status/latest.json"),
            logs: PathBuf::from("/state/rulesyos/logs"),
            lock: PathBuf::from("/state/rulesyos/locks/stage0.lock"),
            baked_config: BAKED_CONFIG.to_vec(),
            rulesyos_version: RULESYOS_VERSION.to_owned(),
            rulesy_version: RULESY_VERSION.to_owned(),
            rulesy_sha256: RULESY_SHA256.to_owned(),
            expected_uid: 0,
            require_root: true,
            timeout: Duration::from_secs(30 * 60),
            term_grace: Duration::from_secs(5),
            output_limit: 256 * 1024,
            log_limit: 8,
        }
    }

    pub(crate) fn rulesy_args(&self) -> Vec<String> {
        vec![
            format!("--config={}", self.config.display()),
            "check".to_owned(),
            "--fix".to_owned(),
            "--non-interactive".to_owned(),
        ]
    }

    pub(crate) fn rulesyos_dir(&self) -> PathBuf {
        self.state.join("rulesyos")
    }

    pub(crate) fn status_dir(&self) -> PathBuf {
        self.status
            .parent()
            .expect("the fixed status path has a parent")
            .to_path_buf()
    }

    pub(crate) fn lock_dir(&self) -> PathBuf {
        self.lock
            .parent()
            .expect("the fixed lock path has a parent")
            .to_path_buf()
    }

    pub(crate) fn config_dir(&self) -> PathBuf {
        self.config
            .parent()
            .expect("the fixed configuration path has a parent")
            .to_path_buf()
    }
}
