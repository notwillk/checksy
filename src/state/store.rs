use crate::state::identity::{GenerationId, SourceId};
use crate::state::integrity::{self, BundleLimits, ValidatedBundle};
use crate::state::model::{
    AuditAction, AuditOutcome, AuditRecord, FailureRecord, Freshness, GenerationMarker,
    StateSnapshot, StateSource,
};
use crate::state_lock::{LockError, StateDirectoryLock};
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

const STATE_JSON_MAX_BYTES: usize = 4 * 1024 * 1024;
const MARKER_OR_AUDIT_MAX_BYTES: usize = 64 * 1024;
const FAILURE_MAX_BYTES: usize = 512 * 1024;
const MAX_FAILURE_RECORDS: usize = 10;
const MAX_AUDIT_RECORDS: usize = 100;
const FAILURE_RETENTION: time::Duration = time::Duration::days(30);
const AUDIT_RETENTION: time::Duration = time::Duration::days(90);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StateScope {
    User,
    System,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StateRootSpec {
    path: PathBuf,
    scope: StateScope,
    expected_uid: u32,
    expected_gid: u32,
}

impl StateRootSpec {
    pub(crate) fn explicit(
        path: PathBuf,
        scope: StateScope,
        expected_uid: u32,
        expected_gid: u32,
    ) -> Result<Self, StoreError> {
        if !path.is_absolute() {
            return Err(StoreError::Integrity(format!(
                "state root '{}' must be absolute",
                path.display()
            )));
        }
        Ok(Self {
            path,
            scope,
            expected_uid,
            expected_gid,
        })
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn for_current_identity(scope: StateScope) -> Result<Self, StoreError> {
        let (path, uid, gid) = match scope {
            StateScope::System => {
                #[cfg(target_os = "linux")]
                {
                    (PathBuf::from("/var/lib/checksy"), 0, 0)
                }
                #[cfg(target_os = "macos")]
                {
                    (PathBuf::from("/Library/Application Support/checksy"), 0, 0)
                }
            }
            StateScope::User => (
                user_state_root()?,
                rustix::process::geteuid().as_raw(),
                rustix::process::getegid().as_raw(),
            ),
        };
        Self::explicit(path, scope, uid, gid)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    fn root_mode(&self) -> u32 {
        match self.scope {
            StateScope::User => 0o700,
            StateScope::System => 0o755,
        }
    }

    fn traversable_mode(&self) -> u32 {
        self.root_mode()
    }

    fn bundle_directory_mode(&self) -> u32 {
        match self.scope {
            StateScope::User => 0o500,
            StateScope::System => 0o555,
        }
    }

    fn bundle_file_mode(&self, executable: bool) -> u32 {
        match (self.scope, executable) {
            (StateScope::User, false) => 0o400,
            (StateScope::User, true) => 0o500,
            (StateScope::System, false) => 0o444,
            (StateScope::System, true) => 0o555,
        }
    }
}

#[cfg(target_os = "linux")]
fn user_state_root() -> Result<PathBuf, StoreError> {
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        let root = PathBuf::from(xdg);
        if !root.is_absolute() {
            return Err(StoreError::Integrity(
                "XDG_STATE_HOME must be absolute".to_string(),
            ));
        }
        return Ok(root.join("checksy"));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| StoreError::Integrity("HOME is required for user state".to_string()))?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        return Err(StoreError::Integrity("HOME must be absolute".to_string()));
    }
    Ok(home.join(".local/state/checksy"))
}

#[cfg(target_os = "macos")]
fn user_state_root() -> Result<PathBuf, StoreError> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| StoreError::Integrity("HOME is required for user state".to_string()))?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        return Err(StoreError::Integrity("HOME must be absolute".to_string()));
    }
    Ok(home.join("Library/Application Support/checksy"))
}

#[derive(Debug)]
pub(crate) enum StoreError {
    Held,
    UnsupportedPlatform,
    SourceUnavailable(String),
    UnsupportedSchemaVersion(u64),
    Integrity(String),
    Io(String),
}

impl StoreError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Held => "lock-held",
            Self::UnsupportedPlatform => "unsupported-platform",
            Self::SourceUnavailable(_) => "source-unavailable",
            Self::UnsupportedSchemaVersion(_) => "unsupported-schema-version",
            Self::Integrity(_) | Self::Io(_) => "state-failed",
        }
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Held => formatter.write_str("state directory lock is already held"),
            Self::UnsupportedPlatform => {
                formatter.write_str("protected state is supported only on Linux and macOS")
            }
            Self::SourceUnavailable(message) | Self::Integrity(message) | Self::Io(message) => {
                formatter.write_str(message)
            }
            Self::UnsupportedSchemaVersion(version) => {
                write!(formatter, "unsupported state schema version {version}")
            }
        }
    }
}

impl std::error::Error for StoreError {}

impl From<LockError> for StoreError {
    fn from(error: LockError) -> Self {
        match error {
            LockError::Held => Self::Held,
            LockError::UnsupportedPlatform => Self::UnsupportedPlatform,
            LockError::State(message) => Self::Io(message),
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[derive(Debug)]
pub(crate) struct StateStore {
    root_fd: rustix::fd::OwnedFd,
    spec: StateRootSpec,
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[derive(Debug)]
pub(crate) struct StateStore {
    spec: StateRootSpec,
}

impl StateStore {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn open(spec: StateRootSpec) -> Result<Self, StoreError> {
        let root_fd = supported::open_root(&spec, false)?;
        supported::validate_root_layout(&root_fd, spec.path(), &spec)?;
        Ok(Self { root_fd, spec })
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn open_or_create(spec: StateRootSpec) -> Result<Self, StoreError> {
        let root_fd = supported::open_root(&spec, true)?;
        supported::validate_root_layout(&root_fd, spec.path(), &spec)?;
        Ok(Self { root_fd, spec })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(crate) fn open(spec: StateRootSpec) -> Result<Self, StoreError> {
        let _ = spec;
        Err(StoreError::UnsupportedPlatform)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(crate) fn open_or_create(spec: StateRootSpec) -> Result<Self, StoreError> {
        let _ = spec;
        Err(StoreError::UnsupportedPlatform)
    }

    pub(crate) fn path(&self) -> &Path {
        self.spec.path()
    }

    /// Test-only trust anchor used because the production opener correctly
    /// rejects world-writable `/tmp` ancestors.
    #[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
    pub(crate) fn from_trusted_test_root(spec: StateRootSpec) -> Result<Self, StoreError> {
        let root_fd = rustix::fs::openat(
            rustix::fs::cwd(),
            spec.path(),
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(|error| {
            StoreError::Io(format!(
                "failed to open trusted test state root '{}': {error}",
                spec.path().display()
            ))
        })?;
        supported::validate_owned_directory(&root_fd, spec.path(), &spec, spec.root_mode())?;
        Ok(Self { root_fd, spec })
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn try_lock(&self) -> Result<LockedStateStore<'_>, StoreError> {
        let lock = StateDirectoryLock::acquire_opened(&self.root_fd, self.path())?;
        rustix::fs::fsync(&self.root_fd).map_err(|error| {
            StoreError::Io(format!(
                "failed to sync protected state root '{}': {error}",
                self.path().display()
            ))
        })?;
        supported::validate_root_layout(&self.root_fd, self.path(), &self.spec)?;
        let locked = LockedStateStore { store: self, lock };
        locked.cleanup_locked_startup()?;
        Ok(locked)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(crate) fn try_lock(&self) -> Result<LockedStateStore<'_>, StoreError> {
        Err(StoreError::UnsupportedPlatform)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn open_source_directory(&self, source_id: SourceId) -> Result<SourceDirectory, StoreError> {
        let sources_path = self.path().join("sources");
        let sources = supported::open_directory(
            &self.root_fd,
            "sources",
            &sources_path,
            &self.spec,
            self.spec.traversable_mode(),
        )?;
        let id = source_id.to_hex();
        let path = sources_path.join(&id);
        let fd = supported::open_directory(
            &sources,
            &id,
            &path,
            &self.spec,
            self.spec.traversable_mode(),
        )?;
        supported::validate_source_layout(&fd, &path, &self.spec)?;
        Ok(SourceDirectory { fd, path })
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn lease_generation_metadata(
        &self,
        source: &StateSource,
        generation_id: GenerationId,
    ) -> Result<GenerationLease, StoreError> {
        let result = (|| {
            let source_dir = self.open_source_directory(source.id)?;
            let generations_path = source_dir.path.join("generations");
            let generations = supported::open_directory(
                &source_dir.fd,
                "generations",
                &generations_path,
                &self.spec,
                self.spec.traversable_mode(),
            )?;
            let id = generation_id.to_hex();
            let generation_path = generations_path.join(&id);
            let generation_fd = supported::open_directory(
                &generations,
                &id,
                &generation_path,
                &self.spec,
                self.spec.traversable_mode(),
            )?;
            supported::require_exact_entries(
                &generation_fd,
                &generation_path,
                &["bundle", "generation.json", "lease"],
            )?;
            let lease_path = generation_path.join("lease");
            let lease_fd =
                supported::open_private_file(&generation_fd, "lease", &lease_path, &self.spec)?;
            supported::acquire_shared_lease(&lease_fd, &lease_path)?;

            let marker_path = generation_path.join("generation.json");
            let bytes = supported::read_private_file(
                &generation_fd,
                "generation.json",
                &marker_path,
                &self.spec,
                MARKER_OR_AUDIT_MAX_BYTES,
            )?;
            let marker = GenerationMarker::decode_strict(&bytes).map_err(map_decode_error)?;
            marker
                .validate_for_source(source)
                .map_err(|error| StoreError::Integrity(error.to_string()))?;
            if marker.generation_id != generation_id {
                return Err(StoreError::Integrity(format!(
                    "generation directory '{}' does not match marker ID {}",
                    id, marker.generation_id
                )));
            }

            let bundle_path = generation_path.join("bundle");
            let bundle_fd = supported::open_directory(
                &generation_fd,
                "bundle",
                &bundle_path,
                &self.spec,
                self.spec.bundle_directory_mode(),
            )?;
            supported::validate_sealed_bundle_tree(
                &bundle_fd,
                &bundle_path,
                &self.spec,
                self.spec.bundle_directory_mode(),
            )?;
            let validated = integrity::validate_bundle_at(
                &bundle_fd,
                marker.config_path.as_str(),
                BundleLimits::default(),
            )
            .map_err(|error| StoreError::Integrity(error.to_string()))?;
            if validated.bundle_sha256 != marker.bundle_sha256 {
                return Err(StoreError::Integrity(format!(
                    "bundle digest mismatch for generation {}: expected {}, found {}",
                    generation_id, marker.bundle_sha256, validated.bundle_sha256
                )));
            }
            Ok(GenerationLease {
                marker,
                validated,
                bundle_path,
                _generation_fd: generation_fd,
                _bundle_fd: bundle_fd,
                _lease_fd: lease_fd,
            })
        })();
        result.map_err(|error| map_internal_absence(error, "completed generation"))
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn lease_generation(
        &self,
        source: &StateSource,
        generation_id: GenerationId,
    ) -> Result<GenerationLease, StoreError> {
        self.lease_generation_metadata(source, generation_id)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn read_snapshot(&self, source_id: SourceId) -> Result<StateSnapshot, StoreError> {
        let source = self.open_source_directory(source_id)?;
        let path = source.path.join("state.json");
        let bytes = supported::read_private_file(
            &source.fd,
            "state.json",
            &path,
            &self.spec,
            STATE_JSON_MAX_BYTES,
        )?;
        let snapshot = StateSnapshot::decode_strict(&bytes).map_err(map_decode_error)?;
        if snapshot.source.id != source_id {
            return Err(StoreError::Integrity(format!(
                "state source ID {} does not match directory {source_id}",
                snapshot.source.id
            )));
        }
        Ok(snapshot)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(crate) fn read_snapshot(&self, _source_id: SourceId) -> Result<StateSnapshot, StoreError> {
        Err(StoreError::UnsupportedPlatform)
    }
}

#[derive(Debug)]
pub(crate) struct LockedStateStore<'a> {
    store: &'a StateStore,
    #[allow(dead_code)]
    lock: StateDirectoryLock,
}

impl LockedStateStore<'_> {
    pub(crate) fn store(&self) -> &StateStore {
        self.store
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn cleanup_locked_startup(&self) -> Result<(), StoreError> {
        if !supported::entry_exists(&self.store.root_fd, "sources", self.store.path())? {
            return Ok(());
        }
        let sources_path = self.store.path().join("sources");
        let sources = supported::open_directory(
            &self.store.root_fd,
            "sources",
            &sources_path,
            &self.store.spec,
            self.store.spec.traversable_mode(),
        )?;
        for name in supported::directory_names(&sources, &sources_path)? {
            let name_text = name.to_str().ok_or_else(|| {
                StoreError::Integrity(format!(
                    "source directory '{}' has a non-UTF-8 entry",
                    sources_path.display()
                ))
            })?;
            if name_text.starts_with(".checksy-tmp-") {
                supported::remove_entry(&sources, &name, &sources_path.join(&name))?;
                continue;
            }
            let source_id = SourceId::parse(name_text).map_err(|error| {
                StoreError::Integrity(format!(
                    "invalid source directory name '{name_text}': {error}"
                ))
            })?;
            let source_path = sources_path.join(name_text);
            let source = supported::open_directory(
                &sources,
                name_text,
                &source_path,
                &self.store.spec,
                self.store.spec.traversable_mode(),
            )?;
            if !supported::entry_exists(&source, "state.json", &source_path)? {
                // `state.json` is written last during initialization, so an
                // owner-protected source without it is an abandoned skeleton.
                supported::remove_opened_directory(&sources, &name, &source, &source_path)?;
                continue;
            }
            let source_directory = SourceDirectory {
                fd: source,
                path: source_path,
            };
            // Remove recognized atomic aliases before strict file-link
            // validation. This recovers the portable macOS no-replace case
            // where a crash occurred after linkat but before temp unlink.
            self.cleanup_source_directories(&source_directory)?;
            self.store.read_snapshot(source_id)?;
        }
        rustix::fs::fsync(&sources).map_err(|error| {
            StoreError::Io(format!(
                "failed to sync sources directory '{}': {error}",
                sources_path.display()
            ))
        })
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn ensure_source_skeleton(&self, source_id: SourceId) -> Result<SourceDirectory, StoreError> {
        let sources_path = self.store.path().join("sources");
        let sources = supported::ensure_directory(
            &self.store.root_fd,
            "sources",
            &sources_path,
            &self.store.spec,
            self.store.spec.traversable_mode(),
        )?;
        let id = source_id.to_hex();
        let path = sources_path.join(&id);
        let source = supported::ensure_directory(
            &sources,
            &id,
            &path,
            &self.store.spec,
            self.store.spec.traversable_mode(),
        )?;

        for (name, mode) in [
            ("trust", 0o700),
            ("generations", self.store.spec.traversable_mode()),
            ("staging", 0o700),
            ("failures", 0o700),
            ("audit", 0o700),
        ] {
            supported::ensure_directory(&source, name, &path.join(name), &self.store.spec, mode)?;
        }
        Ok(SourceDirectory { fd: source, path })
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn clean_orphan_staging(&self, source_id: SourceId) -> Result<(), StoreError> {
        let source = self.store.open_source_directory(source_id)?;
        let path = source.path.join("staging");
        let staging =
            supported::open_directory(&source.fd, "staging", &path, &self.store.spec, 0o700)?;
        supported::remove_tree_contents(&staging, &path)
    }

    /// Initialize a source with the sequence-one, no-current snapshot.
    /// Directory creation is intentionally completed before `state.json`, so
    /// the snapshot remains the sole signal that initialization succeeded.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn initialize_source(&self, snapshot: &StateSnapshot) -> Result<(), StoreError> {
        snapshot
            .validate()
            .map_err(|error| StoreError::Integrity(error.to_string()))?;
        if snapshot.snapshot_sequence.get() != 1
            || snapshot.selection.current.as_ref().is_some()
            || snapshot.selection.previous.as_ref().is_some()
            || !snapshot.selection.additional.is_empty()
            || snapshot.last_attempt.as_ref().is_some()
            || snapshot.last_success.as_ref().is_some()
            || snapshot.last_error.as_ref().is_some()
            || snapshot.recorded_compliance.as_ref().is_some()
            || !freshness_is_initial(&snapshot.freshness)
        {
            return Err(StoreError::Integrity(
                "initial state must be sequence 1 with no selection or attempt metadata"
                    .to_string(),
            ));
        }

        let source = self.ensure_source_skeleton(snapshot.source.id)?;
        self.cleanup_source_directories(&source)?;
        supported::require_exact_entries(
            &source.fd,
            &source.path,
            &["audit", "failures", "generations", "staging", "trust"],
        )?;
        let path = source.path.join("state.json");
        supported::write_json_atomic(
            &source.fd,
            "state.json",
            &path,
            &self.store.spec,
            snapshot,
            STATE_JSON_MAX_BYTES,
            true,
        )
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(crate) fn initialize_source(&self, _snapshot: &StateSnapshot) -> Result<(), StoreError> {
        Err(StoreError::UnsupportedPlatform)
    }

    /// Replace the authoritative snapshot after checking the expected
    /// sequence and leasing every selected immutable generation.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn publish_snapshot(
        &self,
        expected_previous_sequence: u64,
        snapshot: &StateSnapshot,
    ) -> Result<(), StoreError> {
        let previous = self.store.read_snapshot(snapshot.source.id)?;
        if previous.snapshot_sequence.get() != expected_previous_sequence {
            return Err(StoreError::Integrity(format!(
                "stale state update: expected sequence {expected_previous_sequence}, found {}",
                previous.snapshot_sequence.get()
            )));
        }
        snapshot
            .validate_successor(&previous)
            .map_err(|error| StoreError::Integrity(error.to_string()))?;

        // Keep all shared leases live through the metadata rename. This makes
        // selection validation and publication one GC-safe critical section.
        let mut leases = Vec::new();
        for generation in snapshot.selection.generations() {
            let lease = self
                .store
                .lease_generation_metadata(&snapshot.source, generation.generation_id)?;
            generation
                .validate_marker(&snapshot.source, &lease.marker)
                .map_err(|error| StoreError::Integrity(error.to_string()))?;
            if !lease.validated.git_dependencies.is_empty() {
                return Err(StoreError::Integrity(format!(
                    "generation {} has unpinned external Git dependencies and cannot be selected",
                    generation.generation_id
                )));
            }
            leases.push(lease);
        }

        let source = self.store.open_source_directory(snapshot.source.id)?;
        let path = source.path.join("state.json");
        supported::write_json_atomic(
            &source.fd,
            "state.json",
            &path,
            &self.store.spec,
            snapshot,
            STATE_JSON_MAX_BYTES,
            false,
        )
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(crate) fn publish_snapshot(
        &self,
        _expected_previous_sequence: u64,
        _snapshot: &StateSnapshot,
    ) -> Result<(), StoreError> {
        Err(StoreError::UnsupportedPlatform)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn append_audit_record(
        &self,
        source_id: SourceId,
        record: &AuditRecord,
    ) -> Result<(), StoreError> {
        record
            .validate()
            .map_err(|error| StoreError::Integrity(error.to_string()))?;
        if record.source_id != source_id {
            return Err(StoreError::Integrity(
                "audit record sourceId does not match its source directory".to_string(),
            ));
        }
        let source = self.store.open_source_directory(source_id)?;
        let directory_path = source.path.join("audit");
        let directory = supported::open_directory(
            &source.fd,
            "audit",
            &directory_path,
            &self.store.spec,
            0o700,
        )?;
        let name = format!("{}.json", record.audit_id);
        supported::write_json_atomic(
            &directory,
            &name,
            &directory_path.join(&name),
            &self.store.spec,
            record,
            MARKER_OR_AUDIT_MAX_BYTES,
            true,
        )
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn append_failure_record(
        &self,
        source_id: SourceId,
        record: &FailureRecord,
    ) -> Result<(), StoreError> {
        record
            .validate()
            .map_err(|error| StoreError::Integrity(error.to_string()))?;
        let source = self.store.open_source_directory(source_id)?;
        let directory_path = source.path.join("failures");
        let directory = supported::open_directory(
            &source.fd,
            "failures",
            &directory_path,
            &self.store.spec,
            0o700,
        )?;
        let name = format!("{}.json", record.failure_id);
        supported::write_json_atomic(
            &directory,
            &name,
            &directory_path.join(&name),
            &self.store.spec,
            record,
            FAILURE_MAX_BYTES,
            true,
        )
    }

    /// Validate and seal a provider-materialized staging bundle. The
    /// completion marker is written last, but this deliberately leaves the
    /// candidate in `staging/`; P3-2 owns the rename and selection change.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn seal_staging_candidate(
        &self,
        source: &StateSource,
        candidate_id: crate::state::identity::Hash256,
        marker: &GenerationMarker,
    ) -> Result<ValidatedBundle, StoreError> {
        marker
            .validate_for_source(source)
            .map_err(|error| StoreError::Integrity(error.to_string()))?;
        if marker.source_id != source.id {
            return Err(StoreError::Integrity(
                "candidate marker does not belong to its source directory".to_string(),
            ));
        }

        let source_dir = self.ensure_source_skeleton(source.id)?;
        let staging_path = source_dir.path.join("staging");
        let staging = supported::open_directory(
            &source_dir.fd,
            "staging",
            &staging_path,
            &self.store.spec,
            0o700,
        )?;
        let candidate_name = candidate_id.to_hex();
        let candidate_path = staging_path.join(&candidate_name);
        let candidate = supported::open_directory(
            &staging,
            &candidate_name,
            &candidate_path,
            &self.store.spec,
            0o700,
        )?;
        supported::require_exact_entries(&candidate, &candidate_path, &["bundle"])?;
        let bundle_path = candidate_path.join("bundle");
        let bundle =
            supported::open_directory(&candidate, "bundle", &bundle_path, &self.store.spec, 0o700)?;

        let validated = integrity::validate_bundle_at(
            &bundle,
            marker.config_path.as_str(),
            BundleLimits::default(),
        )
        .map_err(|error| StoreError::Integrity(error.to_string()))?;
        if validated.bundle_sha256 != marker.bundle_sha256 {
            return Err(StoreError::Integrity(format!(
                "candidate bundle digest mismatch: expected {}, found {}",
                marker.bundle_sha256, validated.bundle_sha256
            )));
        }

        supported::normalize_bundle_tree(
            &bundle,
            &bundle_path,
            &self.store.spec,
            self.store.spec.bundle_directory_mode(),
        )?;
        let sealed = integrity::validate_bundle_at(
            &bundle,
            marker.config_path.as_str(),
            BundleLimits::default(),
        )
        .map_err(|error| StoreError::Integrity(error.to_string()))?;
        if sealed.bundle_sha256 != validated.bundle_sha256 {
            return Err(StoreError::Integrity(
                "bundle content changed while permissions were sealed".to_string(),
            ));
        }

        let lease_path = candidate_path.join("lease");
        let lease =
            supported::create_private_file(&candidate, "lease", &lease_path, &self.store.spec)?;
        rustix::fs::fsync(&lease).map_err(|error| {
            StoreError::Io(format!(
                "failed to sync generation lease '{}': {error}",
                lease_path.display()
            ))
        })?;
        let marker_path = candidate_path.join("generation.json");
        supported::write_json_atomic(
            &candidate,
            "generation.json",
            &marker_path,
            &self.store.spec,
            marker,
            MARKER_OR_AUDIT_MAX_BYTES,
            true,
        )?;
        supported::require_exact_entries(
            &candidate,
            &candidate_path,
            &["bundle", "generation.json", "lease"],
        )?;
        Ok(sealed)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn garbage_collect(
        &self,
        snapshot: &StateSnapshot,
        now: time::OffsetDateTime,
    ) -> Result<(), StoreError> {
        snapshot
            .validate()
            .map_err(|error| StoreError::Integrity(error.to_string()))?;
        let persisted = self.store.read_snapshot(snapshot.source.id)?;
        if &persisted != snapshot {
            return Err(StoreError::Integrity(
                "garbage collection requires the authoritative state snapshot".to_string(),
            ));
        }
        let mut selected_leases = Vec::new();
        for generation in snapshot.selection.generations() {
            let lease = self
                .store
                .lease_generation_metadata(&snapshot.source, generation.generation_id)?;
            generation
                .validate_marker(&snapshot.source, &lease.marker)
                .map_err(|error| StoreError::Integrity(error.to_string()))?;
            if !lease.validated.git_dependencies.is_empty() {
                return Err(StoreError::Integrity(format!(
                    "selected generation {} has unpinned external Git dependencies",
                    generation.generation_id
                )));
            }
            selected_leases.push(lease);
        }
        let source = self.store.open_source_directory(snapshot.source.id)?;
        self.gc_generations(&source, snapshot)
            .map_err(|error| map_internal_absence(error, "generation history"))?;
        self.gc_failures(&source, now)
            .map_err(|error| map_internal_absence(error, "failure history"))?;
        self.gc_audits(&source, snapshot, now)
            .map_err(|error| map_internal_absence(error, "audit history"))?;
        self.cleanup_source_directories_except_staging(&source)
            .map_err(|error| map_internal_absence(error, "state cleanup"))
    }

    /// P3-1 tests need completed generations in their final location without
    /// giving production code an early promotion primitive. P3-2 replaces
    /// this materializer with the real atomic candidate publication flow.
    #[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
    pub(crate) fn materialize_generation_for_test(
        &self,
        source: &StateSource,
        candidate_id: crate::state::identity::Hash256,
        marker: &GenerationMarker,
    ) -> Result<ValidatedBundle, StoreError> {
        let validated = self.seal_staging_candidate(source, candidate_id, marker)?;
        let source_dir = self.store.open_source_directory(source.id)?;
        let staging_path = source_dir.path.join("staging");
        let staging = supported::open_directory(
            &source_dir.fd,
            "staging",
            &staging_path,
            &self.store.spec,
            0o700,
        )?;
        let generations_path = source_dir.path.join("generations");
        let generations = supported::open_directory(
            &source_dir.fd,
            "generations",
            &generations_path,
            &self.store.spec,
            self.store.spec.traversable_mode(),
        )?;
        let candidate_name = candidate_id.to_hex();
        let candidate_path = staging_path.join(&candidate_name);
        let candidate = supported::open_directory(
            &staging,
            &candidate_name,
            &candidate_path,
            &self.store.spec,
            0o700,
        )?;
        supported::set_directory_mode(
            &candidate,
            &candidate_path,
            self.store.spec.traversable_mode(),
        )?;
        let generation_name = marker.generation_id.to_hex();
        let generation_path = generations_path.join(&generation_name);
        match rustix::fs::statat(
            &generations,
            &generation_name,
            rustix::fs::AtFlags::SYMLINK_NOFOLLOW,
        ) {
            Ok(_) => {
                return Err(StoreError::Integrity(format!(
                    "generation '{}' already exists",
                    generation_path.display()
                )))
            }
            Err(rustix::io::Errno::NOENT) => {}
            Err(error) => {
                return Err(StoreError::Io(format!(
                    "failed to inspect generation '{}': {error}",
                    generation_path.display()
                )))
            }
        }
        rustix::fs::renameat(&staging, &candidate_name, &generations, &generation_name).map_err(
            |error| {
                StoreError::Io(format!(
                    "failed to materialize test generation '{}': {error}",
                    generation_path.display()
                ))
            },
        )?;
        rustix::fs::fsync(&staging).map_err(|error| {
            StoreError::Io(format!(
                "failed to sync staging directory '{}': {error}",
                staging_path.display()
            ))
        })?;
        rustix::fs::fsync(&generations).map_err(|error| {
            StoreError::Io(format!(
                "failed to sync generations directory '{}': {error}",
                generations_path.display()
            ))
        })?;
        Ok(validated)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn gc_generations(
        &self,
        source: &SourceDirectory,
        snapshot: &StateSnapshot,
    ) -> Result<(), StoreError> {
        use std::ffi::OsStr;

        let protected: HashSet<_> = snapshot
            .selection
            .generations()
            .map(|generation| generation.generation_id)
            .collect();
        let path = source.path.join("generations");
        let directory = supported::open_directory(
            &source.fd,
            "generations",
            &path,
            &self.store.spec,
            self.store.spec.traversable_mode(),
        )?;
        for name in supported::directory_names(&directory, &path)? {
            let name_text = name.to_str().ok_or_else(|| {
                StoreError::Integrity(format!(
                    "generation directory '{}' has a non-UTF-8 name",
                    path.display()
                ))
            })?;
            if name_text.starts_with(".checksy-tmp-") {
                supported::remove_entry(&directory, &name, &path.join(&name))?;
                continue;
            }
            let generation_id = GenerationId::parse(name_text).map_err(|error| {
                StoreError::Integrity(format!(
                    "invalid generation directory name '{name_text}': {error}"
                ))
            })?;
            if protected.contains(&generation_id) {
                continue;
            }
            let generation_path = path.join(name_text);
            let generation = supported::open_directory(
                &directory,
                name_text,
                &generation_path,
                &self.store.spec,
                self.store.spec.traversable_mode(),
            )?;
            let marker_path = generation_path.join("generation.json");
            let bytes = supported::read_private_file(
                &generation,
                "generation.json",
                &marker_path,
                &self.store.spec,
                MARKER_OR_AUDIT_MAX_BYTES,
            )?;
            let marker = GenerationMarker::decode_strict(&bytes).map_err(map_decode_error)?;
            marker
                .validate_for_source(&snapshot.source)
                .map_err(|error| StoreError::Integrity(error.to_string()))?;
            if marker.generation_id != generation_id {
                return Err(StoreError::Integrity(format!(
                    "generation directory {generation_id} contains marker {}",
                    marker.generation_id
                )));
            }
            let lease_path = generation_path.join("lease");
            let lease =
                supported::open_private_file(&generation, "lease", &lease_path, &self.store.spec)?;
            if !supported::try_acquire_exclusive_lease(&lease, &lease_path)? {
                continue;
            }
            supported::remove_opened_directory(
                &directory,
                OsStr::new(name_text),
                &generation,
                &generation_path,
            )?;
        }
        rustix::fs::fsync(&directory).map_err(|error| {
            StoreError::Io(format!(
                "failed to sync generations directory '{}': {error}",
                path.display()
            ))
        })
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn gc_failures(
        &self,
        source: &SourceDirectory,
        now: time::OffsetDateTime,
    ) -> Result<(), StoreError> {
        let path = source.path.join("failures");
        let directory =
            supported::open_directory(&source.fd, "failures", &path, &self.store.spec, 0o700)?;
        let mut entries = Vec::new();
        for (name, bytes) in self.read_record_files(&directory, &path, FAILURE_MAX_BYTES)? {
            let record = FailureRecord::decode_strict(&bytes).map_err(map_decode_error)?;
            require_record_filename(&name, &record.failure_id.to_hex())?;
            entries.push(RetentionEntry {
                id: record.failure_id.to_hex(),
                at: timestamp_value(&record.at)?,
                protected: false,
            });
        }
        let retained =
            retained_record_ids(entries.clone(), now, MAX_FAILURE_RECORDS, FAILURE_RETENTION);
        self.remove_unretained_records(&directory, &path, entries, &retained)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn gc_audits(
        &self,
        source: &SourceDirectory,
        snapshot: &StateSnapshot,
        now: time::OffsetDateTime,
    ) -> Result<(), StoreError> {
        let path = source.path.join("audit");
        let directory =
            supported::open_directory(&source.fd, "audit", &path, &self.store.spec, 0o700)?;
        let mut records = Vec::new();
        for (name, bytes) in self.read_record_files(&directory, &path, MARKER_OR_AUDIT_MAX_BYTES)? {
            let record = AuditRecord::decode_strict(&bytes).map_err(map_decode_error)?;
            require_record_filename(&name, &record.audit_id.to_hex())?;
            if record.source_id != snapshot.source.id {
                return Err(StoreError::Integrity(format!(
                    "audit record {} belongs to another source",
                    record.audit_id
                )));
            }
            records.push(record);
        }

        let mut protected = HashSet::new();
        for selected in [
            snapshot.selection.current.as_ref(),
            snapshot.selection.previous.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(record) = newest_audit(records.iter().filter(|record| {
                record.outcome == AuditOutcome::Success
                    && matches!(
                        record.action,
                        AuditAction::Apply | AuditAction::Rollback | AuditAction::Enroll
                    )
                    && record.from_generation_id != record.to_generation_id
                    && record.to_generation_id.as_ref() == Some(&selected.generation_id)
            }))? {
                protected.insert(record.audit_id.to_hex());
            }
        }
        if let Some(record) = newest_audit(
            records
                .iter()
                .filter(|record| record.action == AuditAction::Rollback),
        )? {
            protected.insert(record.audit_id.to_hex());
        }

        let entries: Vec<_> = records
            .iter()
            .map(|record| {
                Ok(RetentionEntry {
                    id: record.audit_id.to_hex(),
                    at: timestamp_value(&record.at)?,
                    protected: protected.contains(&record.audit_id.to_hex()),
                })
            })
            .collect::<Result<_, StoreError>>()?;
        let retained =
            retained_record_ids(entries.clone(), now, MAX_AUDIT_RECORDS, AUDIT_RETENTION);
        self.remove_unretained_records(&directory, &path, entries, &retained)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn read_record_files(
        &self,
        directory: &impl rustix::fd::AsFd,
        path: &Path,
        maximum_bytes: usize,
    ) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        let mut records = Vec::new();
        for name in supported::directory_names(directory, path)? {
            let name = name.into_string().map_err(|_| {
                StoreError::Integrity(format!(
                    "record directory '{}' contains a non-UTF-8 filename",
                    path.display()
                ))
            })?;
            if name.starts_with(".checksy-tmp-") {
                continue;
            }
            let bytes = supported::read_private_file(
                directory,
                &name,
                &path.join(&name),
                &self.store.spec,
                maximum_bytes,
            )?;
            records.push((name, bytes));
        }
        Ok(records)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn remove_unretained_records(
        &self,
        directory: &impl rustix::fd::AsFd,
        path: &Path,
        entries: Vec<RetentionEntry>,
        retained: &HashSet<String>,
    ) -> Result<(), StoreError> {
        for entry in entries {
            if !retained.contains(&entry.id) {
                let name = format!("{}.json", entry.id);
                supported::remove_entry(directory, std::ffi::OsStr::new(&name), &path.join(&name))?;
            }
        }
        rustix::fs::fsync(directory).map_err(|error| {
            StoreError::Io(format!(
                "failed to sync record directory '{}': {error}",
                path.display()
            ))
        })
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn cleanup_source_directories_except_staging(
        &self,
        source: &SourceDirectory,
    ) -> Result<(), StoreError> {
        for (name, mode) in [
            ("audit", 0o700),
            ("failures", 0o700),
            ("generations", self.store.spec.traversable_mode()),
        ] {
            let path = source.path.join(name);
            let directory =
                supported::open_directory(&source.fd, name, &path, &self.store.spec, mode)?;
            supported::remove_recognized_temporaries(&directory, &path)?;
        }
        supported::remove_recognized_temporaries(&source.fd, &source.path)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn cleanup_source_directories(&self, source: &SourceDirectory) -> Result<(), StoreError> {
        let staging_path = source.path.join("staging");
        let staging = supported::open_directory(
            &source.fd,
            "staging",
            &staging_path,
            &self.store.spec,
            0o700,
        )?;
        supported::remove_tree_contents(&staging, &staging_path)?;
        for (name, mode) in [
            ("audit", 0o700),
            ("failures", 0o700),
            ("generations", self.store.spec.traversable_mode()),
        ] {
            let path = source.path.join(name);
            let directory =
                supported::open_directory(&source.fd, name, &path, &self.store.spec, mode)?;
            supported::remove_recognized_temporaries(&directory, &path)?;
        }
        supported::remove_recognized_temporaries(&source.fd, &source.path)
    }
}

fn freshness_is_initial(freshness: &Freshness) -> bool {
    match freshness {
        Freshness::Local { snapshot_sha256 } => snapshot_sha256.is_null(),
        Freshness::Git {
            accepted_commit,
            accepted_tag_object,
            accepted_at,
            ..
        } => accepted_commit.is_null() && accepted_tag_object.is_null() && accepted_at.is_null(),
        Freshness::Https {
            high_water_generation,
            manifest_sha256,
            revision,
            artifact_sha256,
            etag,
            last_modified,
            last_online_contact,
        } => {
            high_water_generation.is_null()
                && manifest_sha256.is_null()
                && revision.is_null()
                && artifact_sha256.is_null()
                && etag.is_null()
                && last_modified.is_null()
                && last_online_contact.is_null()
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[derive(Debug)]
struct SourceDirectory {
    fd: rustix::fd::OwnedFd,
    path: PathBuf,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[derive(Debug)]
pub(crate) struct GenerationLease {
    pub(crate) marker: GenerationMarker,
    pub(crate) validated: ValidatedBundle,
    pub(crate) bundle_path: PathBuf,
    _generation_fd: rustix::fd::OwnedFd,
    _bundle_fd: rustix::fd::OwnedFd,
    _lease_fd: rustix::fd::OwnedFd,
}

fn map_decode_error(error: crate::state::model::DecodeError) -> StoreError {
    match error.unsupported_schema_version() {
        Some(version) => StoreError::UnsupportedSchemaVersion(version),
        None => StoreError::Integrity(error.to_string()),
    }
}

fn map_internal_absence(error: StoreError, context: &str) -> StoreError {
    match error {
        StoreError::SourceUnavailable(message) => StoreError::Integrity(format!(
            "{context} is missing from protected state: {message}"
        )),
        other => other,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RetentionEntry {
    id: String,
    at: time::OffsetDateTime,
    protected: bool,
}

fn retained_record_ids(
    mut entries: Vec<RetentionEntry>,
    now: time::OffsetDateTime,
    maximum_ordinary: usize,
    maximum_age: time::Duration,
) -> std::collections::HashSet<String> {
    entries.sort_by(|left, right| right.at.cmp(&left.at).then_with(|| left.id.cmp(&right.id)));

    let mut ordinary = 0_usize;
    entries
        .into_iter()
        .filter_map(|entry| {
            if entry.protected {
                return Some(entry.id);
            }
            let age = now - entry.at;
            if age > maximum_age || ordinary >= maximum_ordinary {
                return None;
            }
            ordinary += 1;
            Some(entry.id)
        })
        .collect()
}

fn timestamp_value(
    timestamp: &crate::state::model::Timestamp,
) -> Result<time::OffsetDateTime, StoreError> {
    time::OffsetDateTime::parse(
        timestamp.as_str(),
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|error| StoreError::Integrity(format!("invalid persisted timestamp: {error}")))
}

fn require_record_filename(name: &str, expected_id: &str) -> Result<(), StoreError> {
    let expected = format!("{expected_id}.json");
    if name != expected {
        return Err(StoreError::Integrity(format!(
            "record filename '{name}' does not match record ID {expected_id}"
        )));
    }
    Ok(())
}

fn newest_audit<'a>(
    records: impl Iterator<Item = &'a AuditRecord>,
) -> Result<Option<&'a AuditRecord>, StoreError> {
    let mut newest: Option<(&AuditRecord, time::OffsetDateTime)> = None;
    for record in records {
        let at = timestamp_value(&record.at)?;
        let replace = newest.as_ref().is_none_or(|(current, current_at)| {
            at > *current_at
                || (at == *current_at && record.audit_id.to_hex() < current.audit_id.to_hex())
        });
        if replace {
            newest = Some((record, at));
        }
    }
    Ok(newest.map(|(record, _)| record))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod supported {
    use super::{StateRootSpec, StoreError};
    use rustix::fd::OwnedFd;
    use rustix::fs::{self, FileType, FlockOperation, Mode, OFlags};
    use rustix::io::{retry_on_intr, Errno};
    use std::ffi::{OsStr, OsString};
    use std::io::{Read, Write};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::{Component, Path};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum AtomicWriteBoundary {
        TemporaryCreated,
        DataWritten,
        FileSynced,
        Renamed,
        DirectorySynced,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct AtomicWritePolicy {
        pub(super) maximum_bytes: usize,
        pub(super) reject_existing: bool,
    }

    const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::NONBLOCK)
        .union(OFlags::CLOEXEC);

    pub(super) fn open_root(
        spec: &StateRootSpec,
        create_missing: bool,
    ) -> Result<OwnedFd, StoreError> {
        if !spec.path.is_absolute() {
            return Err(StoreError::Integrity(format!(
                "state root '{}' must be absolute",
                spec.path.display()
            )));
        }

        let mut current = fs::openat(fs::cwd(), Path::new("/"), DIRECTORY_FLAGS, Mode::empty())
            .map_err(|error| io_error("open", Path::new("/"), error))?;
        validate_ancestor(&current, Path::new("/"), spec)?;

        let mut components = Vec::new();
        for component in spec.path.components() {
            match component {
                Component::RootDir => {}
                Component::Normal(name) => components.push(name.to_os_string()),
                _ => {
                    return Err(StoreError::Integrity(format!(
                        "state root '{}' must be component-normalized",
                        spec.path.display()
                    )))
                }
            }
        }
        if components.is_empty() {
            return Err(StoreError::Integrity(
                "filesystem root cannot be used as the Checksy state root".to_string(),
            ));
        }

        let mut display = Path::new("/").to_path_buf();
        for (index, component) in components.iter().enumerate() {
            if Path::new(component).components().count() != 1 {
                return Err(StoreError::Integrity(format!(
                    "invalid state path component in '{}'",
                    spec.path.display()
                )));
            }
            display.push(component);
            let is_root = index + 1 == components.len();
            let opened = match fs::openat(&current, component, DIRECTORY_FLAGS, Mode::empty()) {
                Ok(fd) => fd,
                Err(Errno::NOENT) if create_missing => {
                    let mode = Mode::from_raw_mode(spec.root_mode());
                    match fs::mkdirat(&current, component, mode) {
                        Ok(()) | Err(Errno::EXIST) => {}
                        Err(error) => return Err(io_error("create", &display, error)),
                    }
                    let fd = fs::openat(&current, component, DIRECTORY_FLAGS, Mode::empty())
                        .map_err(|error| io_error("open", &display, error))?;
                    fs::fchmod(&fd, mode)
                        .map_err(|error| io_error("set permissions on", &display, error))?;
                    fs::fsync(&fd).map_err(|error| io_error("sync", &display, error))?;
                    fs::fsync(&current)
                        .map_err(|error| io_error("sync parent of", &display, error))?;
                    fd
                }
                Err(Errno::NOENT) => {
                    return Err(StoreError::SourceUnavailable(format!(
                        "state root '{}' does not exist",
                        spec.path.display()
                    )))
                }
                Err(error) => return Err(io_error("open", &display, error)),
            };
            if is_root {
                validate_owned_directory(&opened, &display, spec, spec.root_mode())?;
            } else {
                validate_ancestor(&opened, &display, spec)?;
            }
            current = opened;
        }
        Ok(current)
    }

    fn validate_ancestor(
        fd: &OwnedFd,
        path: &Path,
        spec: &StateRootSpec,
    ) -> Result<(), StoreError> {
        let stat = fs::fstat(fd).map_err(|error| io_error("inspect", path, error))?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::Directory {
            return Err(StoreError::Integrity(format!(
                "state ancestor '{}' is not a directory",
                path.display()
            )));
        }
        if stat.st_uid != 0 && stat.st_uid != spec.expected_uid {
            return Err(StoreError::Integrity(format!(
                "state ancestor '{}' is owned by uid {}, expected root or uid {}",
                path.display(),
                stat.st_uid,
                spec.expected_uid
            )));
        }
        if stat.st_mode & 0o022 != 0 {
            return Err(StoreError::Integrity(format!(
                "state ancestor '{}' must not be group- or world-writable",
                path.display()
            )));
        }
        Ok(())
    }

    pub(super) fn validate_owned_directory(
        fd: &OwnedFd,
        path: &Path,
        spec: &StateRootSpec,
        required_mode: u32,
    ) -> Result<(), StoreError> {
        let stat = fs::fstat(fd).map_err(|error| io_error("inspect", path, error))?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::Directory {
            return Err(StoreError::Integrity(format!(
                "state path '{}' is not a directory",
                path.display()
            )));
        }
        if stat.st_uid != spec.expected_uid || stat.st_gid != spec.expected_gid {
            return Err(StoreError::Integrity(format!(
                "state directory '{}' must be owned by uid {} and gid {}; found uid {} and gid {}",
                path.display(),
                spec.expected_uid,
                spec.expected_gid,
                stat.st_uid,
                stat.st_gid
            )));
        }
        let found = stat.st_mode & 0o7777;
        if found != required_mode {
            return Err(StoreError::Integrity(format!(
                "state directory '{}' must have mode {:04o}, found {:04o}",
                path.display(),
                required_mode,
                found
            )));
        }
        Ok(())
    }

    pub(super) fn require_exact_entries(
        directory: &impl rustix::fd::AsFd,
        display_path: &Path,
        expected: &[&str],
    ) -> Result<(), StoreError> {
        let mut actual = directory_names(directory, display_path)?;
        let mut expected: Vec<_> = expected.iter().map(OsString::from).collect();
        actual.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        expected.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        if actual != expected {
            return Err(StoreError::Integrity(format!(
                "protected directory '{}' does not have the required closed layout",
                display_path.display()
            )));
        }
        Ok(())
    }

    pub(super) fn validate_source_layout(
        source: &impl rustix::fd::AsFd,
        display_path: &Path,
        spec: &StateRootSpec,
    ) -> Result<(), StoreError> {
        let names = directory_names(source, display_path)?;
        let required = [
            "audit",
            "failures",
            "generations",
            "staging",
            "state.json",
            "trust",
        ];
        for required_name in required {
            if !names
                .iter()
                .any(|name| name.as_bytes() == required_name.as_bytes())
            {
                return Err(StoreError::Integrity(format!(
                    "source state '{}' is missing required entry '{required_name}'",
                    display_path.display()
                )));
            }
        }
        for name in &names {
            let bytes = name.as_bytes();
            if !required.iter().any(|allowed| bytes == allowed.as_bytes())
                && bytes != b"policy.json"
                && !bytes.starts_with(b".checksy-tmp-")
            {
                return Err(StoreError::Integrity(format!(
                    "source state '{}' contains unexpected entry '{}'",
                    display_path.display(),
                    name.to_string_lossy()
                )));
            }
        }
        for (name, mode) in [
            ("trust", 0o700),
            ("staging", 0o700),
            ("failures", 0o700),
            ("audit", 0o700),
            ("generations", spec.traversable_mode()),
        ] {
            let _ = open_directory(source, name, &display_path.join(name), spec, mode)?;
        }
        if names.iter().any(|name| name.as_bytes() == b"policy.json") {
            let policy_path = display_path.join("policy.json");
            let bytes = read_private_file(source, "policy.json", &policy_path, spec, 64 * 1024)?;
            if bytes.is_empty() {
                return Err(StoreError::Integrity(format!(
                    "protected policy '{}' is empty",
                    policy_path.display()
                )));
            }
        }
        Ok(())
    }

    pub(super) fn validate_root_layout(
        root: &impl rustix::fd::AsFd,
        display_path: &Path,
        spec: &StateRootSpec,
    ) -> Result<(), StoreError> {
        let names = directory_names(root, display_path)?;
        for name in &names {
            if !matches!(name.as_bytes(), b"lock" | b"sources") {
                return Err(StoreError::Integrity(format!(
                    "protected state root '{}' contains unexpected entry '{}'",
                    display_path.display(),
                    name.to_string_lossy()
                )));
            }
        }
        if names.iter().any(|name| name.as_bytes() == b"lock") {
            let _ = open_private_file(root, "lock", &display_path.join("lock"), spec)?;
        }
        if names.iter().any(|name| name.as_bytes() == b"sources") {
            let _ = open_directory(
                root,
                "sources",
                &display_path.join("sources"),
                spec,
                spec.traversable_mode(),
            )?;
        }
        Ok(())
    }

    pub(super) fn set_directory_mode(
        fd: &impl rustix::fd::AsFd,
        path: &Path,
        mode: u32,
    ) -> Result<(), StoreError> {
        fs::fchmod(fd, Mode::from_raw_mode(mode))
            .map_err(|error| io_error("set permissions on", path, error))
    }

    pub(super) fn ensure_directory(
        parent: &impl rustix::fd::AsFd,
        name: &str,
        display_path: &Path,
        spec: &StateRootSpec,
        mode: u32,
    ) -> Result<OwnedFd, StoreError> {
        let flags = DIRECTORY_FLAGS;
        let fd = match fs::openat(parent, name, flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(Errno::NOENT) => {
                let required = Mode::from_raw_mode(mode);
                match fs::mkdirat(parent, name, required) {
                    Ok(()) | Err(Errno::EXIST) => {}
                    Err(error) => return Err(io_error("create", display_path, error)),
                }
                let fd = fs::openat(parent, name, flags, Mode::empty())
                    .map_err(|error| io_error("open", display_path, error))?;
                fs::fchmod(&fd, required)
                    .map_err(|error| io_error("set permissions on", display_path, error))?;
                fs::fsync(&fd).map_err(|error| io_error("sync", display_path, error))?;
                fs::fsync(parent)
                    .map_err(|error| io_error("sync parent of", display_path, error))?;
                fd
            }
            Err(error) => return Err(io_error("open", display_path, error)),
        };
        validate_owned_directory(&fd, display_path, spec, mode)?;
        Ok(fd)
    }

    pub(super) fn open_directory(
        parent: &impl rustix::fd::AsFd,
        name: &str,
        display_path: &Path,
        spec: &StateRootSpec,
        mode: u32,
    ) -> Result<OwnedFd, StoreError> {
        let fd =
            fs::openat(parent, name, DIRECTORY_FLAGS, Mode::empty()).map_err(
                |error| match error {
                    Errno::NOENT => StoreError::SourceUnavailable(format!(
                        "protected state directory '{}' does not exist",
                        display_path.display()
                    )),
                    other => io_error("open", display_path, other),
                },
            )?;
        validate_owned_directory(&fd, display_path, spec, mode)?;
        Ok(fd)
    }

    pub(super) fn entry_exists(
        parent: &impl rustix::fd::AsFd,
        name: &str,
        display_parent: &Path,
    ) -> Result<bool, StoreError> {
        match fs::statat(parent, name, fs::AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => Ok(true),
            Err(Errno::NOENT) => Ok(false),
            Err(error) => Err(io_error("inspect", &display_parent.join(name), error)),
        }
    }

    pub(super) fn read_private_file(
        parent: &impl rustix::fd::AsFd,
        name: &str,
        display_path: &Path,
        spec: &StateRootSpec,
        maximum_bytes: usize,
    ) -> Result<Vec<u8>, StoreError> {
        let fd = fs::openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| match error {
            Errno::NOENT => StoreError::SourceUnavailable(format!(
                "protected state file '{}' does not exist",
                display_path.display()
            )),
            other => io_error("open", display_path, other),
        })?;
        validate_private_file(&fd, display_path, spec, 0o600)?;
        let stat = fs::fstat(&fd).map_err(|error| io_error("inspect", display_path, error))?;
        if stat.st_size < 0 || stat.st_size as u64 > maximum_bytes as u64 {
            return Err(StoreError::Integrity(format!(
                "protected state file '{}' exceeds {} bytes",
                display_path.display(),
                maximum_bytes
            )));
        }
        let mut file = std::fs::File::from(fd);
        let mut bytes = Vec::with_capacity(stat.st_size as usize);
        std::io::Read::by_ref(&mut file)
            .take(maximum_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                StoreError::Io(format!(
                    "failed to read protected state '{}': {error}",
                    display_path.display()
                ))
            })?;
        if bytes.len() > maximum_bytes {
            return Err(StoreError::Integrity(format!(
                "protected state file '{}' exceeds {} bytes",
                display_path.display(),
                maximum_bytes
            )));
        }
        Ok(bytes)
    }

    pub(super) fn open_private_file(
        parent: &impl rustix::fd::AsFd,
        name: &str,
        display_path: &Path,
        spec: &StateRootSpec,
    ) -> Result<OwnedFd, StoreError> {
        let fd = fs::openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| match error {
            Errno::NOENT => StoreError::SourceUnavailable(format!(
                "protected state file '{}' does not exist",
                display_path.display()
            )),
            other => io_error("open", display_path, other),
        })?;
        validate_private_file(&fd, display_path, spec, 0o600)?;
        Ok(fd)
    }

    pub(super) fn create_private_file(
        parent: &impl rustix::fd::AsFd,
        name: &str,
        display_path: &Path,
        spec: &StateRootSpec,
    ) -> Result<OwnedFd, StoreError> {
        let fd = fs::openat(
            parent,
            name,
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|error| io_error("create", display_path, error))?;
        fs::fchmod(&fd, Mode::RUSR | Mode::WUSR)
            .map_err(|error| io_error("set permissions on", display_path, error))?;
        validate_private_file(&fd, display_path, spec, 0o600)?;
        Ok(fd)
    }

    pub(super) fn acquire_shared_lease(
        lease_fd: &impl rustix::fd::AsFd,
        display_path: &Path,
    ) -> Result<(), StoreError> {
        retry_on_intr(|| fs::flock(lease_fd, FlockOperation::LockShared))
            .map_err(|error| io_error("acquire shared generation lease on", display_path, error))
    }

    pub(super) fn try_acquire_exclusive_lease(
        lease_fd: &impl rustix::fd::AsFd,
        display_path: &Path,
    ) -> Result<bool, StoreError> {
        loop {
            match fs::flock(lease_fd, FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => return Ok(true),
                Err(Errno::INTR) => continue,
                Err(Errno::AGAIN) => return Ok(false),
                Err(error) => {
                    return Err(io_error(
                        "acquire exclusive generation lease on",
                        display_path,
                        error,
                    ))
                }
            }
        }
    }

    pub(super) fn normalize_bundle_tree(
        directory: &impl rustix::fd::AsFd,
        display_path: &Path,
        spec: &StateRootSpec,
        directory_mode: u32,
    ) -> Result<(), StoreError> {
        let directory_stat =
            fs::fstat(directory).map_err(|error| io_error("inspect", display_path, error))?;
        if FileType::from_raw_mode(directory_stat.st_mode) != FileType::Directory
            || directory_stat.st_uid != spec.expected_uid
            || directory_stat.st_gid != spec.expected_gid
        {
            return Err(StoreError::Integrity(format!(
                "bundle directory '{}' has unexpected type or ownership",
                display_path.display()
            )));
        }

        for name in directory_names(directory, display_path)? {
            let path = display_path.join(&name);
            let stat = fs::statat(directory, &name, fs::AtFlags::SYMLINK_NOFOLLOW)
                .map_err(|error| io_error("inspect", &path, error))?;
            match FileType::from_raw_mode(stat.st_mode) {
                FileType::Directory => {
                    let child = fs::openat(directory, &name, DIRECTORY_FLAGS, Mode::empty())
                        .map_err(|error| io_error("open", &path, error))?;
                    normalize_bundle_tree(&child, &path, spec, directory_mode)?;
                }
                FileType::RegularFile => {
                    if stat.st_nlink != 1
                        || stat.st_uid != spec.expected_uid
                        || stat.st_gid != spec.expected_gid
                    {
                        return Err(StoreError::Integrity(format!(
                            "bundle file '{}' has unexpected ownership or hard links",
                            path.display()
                        )));
                    }
                    let executable = stat.st_mode & 0o111 != 0;
                    let file = fs::openat(
                        directory,
                        &name,
                        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(|error| io_error("open", &path, error))?;
                    let mode = Mode::from_raw_mode(spec.bundle_file_mode(executable));
                    fs::fchmod(&file, mode)
                        .map_err(|error| io_error("seal permissions on", &path, error))?;
                    fs::fsync(&file).map_err(|error| io_error("sync", &path, error))?;
                }
                _ => {
                    return Err(StoreError::Integrity(format!(
                        "bundle entry '{}' is not a directory or regular file",
                        path.display()
                    )))
                }
            }
        }
        fs::fchmod(directory, Mode::from_raw_mode(directory_mode))
            .map_err(|error| io_error("seal permissions on", display_path, error))?;
        fs::fsync(directory).map_err(|error| io_error("sync", display_path, error))
    }

    pub(super) fn validate_sealed_bundle_tree(
        directory: &impl rustix::fd::AsFd,
        display_path: &Path,
        spec: &StateRootSpec,
        directory_mode: u32,
    ) -> Result<(), StoreError> {
        validate_owned_directory_descriptor(directory, display_path, spec, directory_mode)?;
        for name in directory_names(directory, display_path)? {
            let path = display_path.join(&name);
            let stat = fs::statat(directory, &name, fs::AtFlags::SYMLINK_NOFOLLOW)
                .map_err(|error| io_error("inspect", &path, error))?;
            match FileType::from_raw_mode(stat.st_mode) {
                FileType::Directory => {
                    let child = fs::openat(directory, &name, DIRECTORY_FLAGS, Mode::empty())
                        .map_err(|error| io_error("open", &path, error))?;
                    validate_sealed_bundle_tree(&child, &path, spec, directory_mode)?;
                }
                FileType::RegularFile => {
                    let executable = stat.st_mode & 0o111 != 0;
                    let file = fs::openat(
                        directory,
                        &name,
                        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(|error| io_error("open", &path, error))?;
                    validate_private_file(&file, &path, spec, spec.bundle_file_mode(executable))?;
                }
                _ => {
                    return Err(StoreError::Integrity(format!(
                        "sealed bundle entry '{}' is not a directory or regular file",
                        path.display()
                    )))
                }
            }
        }
        Ok(())
    }

    fn validate_owned_directory_descriptor(
        fd: &impl rustix::fd::AsFd,
        path: &Path,
        spec: &StateRootSpec,
        required_mode: u32,
    ) -> Result<(), StoreError> {
        let stat = fs::fstat(fd).map_err(|error| io_error("inspect", path, error))?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::Directory
            || stat.st_uid != spec.expected_uid
            || stat.st_gid != spec.expected_gid
            || stat.st_mode & 0o7777 != required_mode
        {
            return Err(StoreError::Integrity(format!(
                "sealed bundle directory '{}' has unexpected type, ownership, or mode",
                path.display()
            )));
        }
        Ok(())
    }

    pub(super) fn write_json_atomic<T: serde::Serialize>(
        parent: &impl rustix::fd::AsFd,
        final_name: &str,
        display_path: &Path,
        spec: &StateRootSpec,
        value: &T,
        maximum_bytes: usize,
        reject_existing: bool,
    ) -> Result<(), StoreError> {
        write_json_atomic_observed(
            parent,
            final_name,
            display_path,
            spec,
            value,
            AtomicWritePolicy {
                maximum_bytes,
                reject_existing,
            },
            |_| Ok(()),
        )
    }

    pub(super) fn write_json_atomic_observed<T: serde::Serialize>(
        parent: &impl rustix::fd::AsFd,
        final_name: &str,
        display_path: &Path,
        spec: &StateRootSpec,
        value: &T,
        policy: AtomicWritePolicy,
        mut observe: impl FnMut(AtomicWriteBoundary) -> Result<(), StoreError>,
    ) -> Result<(), StoreError> {
        let AtomicWritePolicy {
            maximum_bytes,
            reject_existing,
        } = policy;
        let mut bytes = serde_json::to_vec(value).map_err(|error| {
            StoreError::Integrity(format!(
                "failed to serialize protected state '{}': {error}",
                display_path.display()
            ))
        })?;
        bytes.push(b'\n');
        if bytes.len() > maximum_bytes {
            return Err(StoreError::Integrity(format!(
                "serialized protected state '{}' exceeds {} bytes",
                display_path.display(),
                maximum_bytes
            )));
        }

        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary_name = format!(".checksy-tmp-{}-{sequence}", std::process::id());
        let temporary_path = display_path.with_file_name(&temporary_name);
        let fd = fs::openat(
            parent,
            temporary_name.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|error| io_error("create temporary file for", &temporary_path, error))?;
        if let Err(error) = fs::fchmod(&fd, Mode::RUSR | Mode::WUSR) {
            let _ = fs::unlinkat(parent, temporary_name.as_str(), fs::AtFlags::empty());
            return Err(io_error("set permissions on", &temporary_path, error));
        }
        if let Err(error) = validate_private_file(&fd, &temporary_path, spec, 0o600) {
            let _ = fs::unlinkat(parent, temporary_name.as_str(), fs::AtFlags::empty());
            return Err(error);
        }

        let write_result = (|| {
            observe(AtomicWriteBoundary::TemporaryCreated)?;
            let mut file = std::fs::File::from(fd);
            file.write_all(&bytes).map_err(|error| {
                StoreError::Io(format!(
                    "failed to write protected state '{}': {error}",
                    temporary_path.display()
                ))
            })?;
            file.flush().map_err(|error| {
                StoreError::Io(format!(
                    "failed to flush protected state '{}': {error}",
                    temporary_path.display()
                ))
            })?;
            observe(AtomicWriteBoundary::DataWritten)?;
            fs::fsync(&file).map_err(|error| io_error("sync", &temporary_path, error))?;
            observe(AtomicWriteBoundary::FileSynced)?;
            if reject_existing {
                publish_no_replace(parent, temporary_name.as_str(), final_name, display_path)?;
                validate_private_file(&file, display_path, spec, 0o600)?;
            } else {
                fs::renameat(parent, temporary_name.as_str(), parent, final_name)
                    .map_err(|error| io_error("publish", display_path, error))?;
            }
            observe(AtomicWriteBoundary::Renamed)?;
            fs::fsync(parent)
                .map_err(|error| io_error("sync directory for", display_path, error))?;
            observe(AtomicWriteBoundary::DirectorySynced)
        })();

        if write_result.is_err() {
            // If rename already succeeded this simply observes ENOENT, leaving
            // the complete new file in place while still reporting the failed
            // durability step.
            let _ = fs::unlinkat(parent, temporary_name.as_str(), fs::AtFlags::empty());
        }
        write_result
    }

    fn publish_no_replace(
        parent: &impl rustix::fd::AsFd,
        temporary_name: &str,
        final_name: &str,
        display_path: &Path,
    ) -> Result<(), StoreError> {
        #[cfg(target_os = "linux")]
        {
            match fs::renameat_with(
                parent,
                temporary_name,
                parent,
                final_name,
                fs::RenameFlags::NOREPLACE,
            ) {
                Ok(()) => return Ok(()),
                Err(Errno::EXIST) => {
                    return Err(StoreError::Integrity(format!(
                        "protected state record '{}' already exists",
                        display_path.display()
                    )))
                }
                Err(Errno::NOSYS) | Err(Errno::INVAL) => {}
                Err(error) => {
                    return Err(io_error("publish without replacement", display_path, error))
                }
            }
        }

        // macOS lacks renameat2(RENAME_NOREPLACE); an exclusive hard link is
        // the portable same-filesystem no-replace publication primitive. A
        // crash before unlink leaves a recognized complete temporary alias,
        // which locked startup removes before accepting the final file.
        fs::linkat(
            parent,
            temporary_name,
            parent,
            final_name,
            fs::AtFlags::empty(),
        )
        .map_err(|error| match error {
            Errno::EXIST => StoreError::Integrity(format!(
                "protected state record '{}' already exists",
                display_path.display()
            )),
            other => io_error("publish without replacement", display_path, other),
        })?;
        fs::unlinkat(parent, temporary_name, fs::AtFlags::empty())
            .map_err(|error| io_error("remove published temporary for", display_path, error))
    }

    pub(super) fn validate_private_file(
        fd: &impl rustix::fd::AsFd,
        path: &Path,
        spec: &StateRootSpec,
        required_mode: u32,
    ) -> Result<(), StoreError> {
        let stat = fs::fstat(fd).map_err(|error| io_error("inspect", path, error))?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
            return Err(StoreError::Integrity(format!(
                "protected state file '{}' is not a regular file",
                path.display()
            )));
        }
        if stat.st_nlink != 1 {
            return Err(StoreError::Integrity(format!(
                "protected state file '{}' must have exactly one hard link",
                path.display()
            )));
        }
        if stat.st_uid != spec.expected_uid || stat.st_gid != spec.expected_gid {
            return Err(StoreError::Integrity(format!(
                "protected state file '{}' has unexpected ownership",
                path.display()
            )));
        }
        let found = stat.st_mode & 0o7777;
        if found != required_mode {
            return Err(StoreError::Integrity(format!(
                "protected state file '{}' must have mode {:04o}, found {:04o}",
                path.display(),
                required_mode,
                found
            )));
        }
        Ok(())
    }

    pub(super) fn directory_names(
        directory: &impl rustix::fd::AsFd,
        display_path: &Path,
    ) -> Result<Vec<OsString>, StoreError> {
        let mut reader = fs::Dir::read_from(directory)
            .map_err(|error| io_error("read directory", display_path, error))?;
        let mut names = Vec::new();
        while let Some(entry) = reader.read() {
            let entry = entry.map_err(|error| io_error("read directory", display_path, error))?;
            let bytes = entry.file_name().to_bytes();
            if bytes == b"." || bytes == b".." {
                continue;
            }
            names.push(OsString::from_vec(bytes.to_vec()));
        }
        names.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        Ok(names)
    }

    pub(super) fn remove_tree_contents(
        directory: &impl rustix::fd::AsFd,
        display_path: &Path,
    ) -> Result<(), StoreError> {
        for name in directory_names(directory, display_path)? {
            remove_entry(directory, &name, &display_path.join(&name))?;
        }
        fs::fsync(directory).map_err(|error| io_error("sync directory", display_path, error))
    }

    pub(super) fn remove_recognized_temporaries(
        directory: &impl rustix::fd::AsFd,
        display_path: &Path,
    ) -> Result<(), StoreError> {
        for name in directory_names(directory, display_path)? {
            if name.as_bytes().starts_with(b".checksy-tmp-") {
                remove_entry(directory, &name, &display_path.join(&name))?;
            }
        }
        fs::fsync(directory).map_err(|error| io_error("sync directory", display_path, error))
    }

    pub(super) fn remove_entry(
        parent: &impl rustix::fd::AsFd,
        name: &OsStr,
        display_path: &Path,
    ) -> Result<(), StoreError> {
        let stat = fs::statat(parent, name, fs::AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|error| io_error("inspect", display_path, error))?;
        let parent_stat = fs::fstat(parent)
            .map_err(|error| io_error("inspect parent of", display_path, error))?;
        if stat.st_uid != rustix::process::geteuid().as_raw()
            || stat.st_gid != rustix::process::getegid().as_raw()
            || (FileType::from_raw_mode(stat.st_mode) == FileType::Directory
                && stat.st_dev != parent_stat.st_dev)
        {
            return Err(StoreError::Integrity(format!(
                "refusing to remove protected entry '{}' with foreign ownership or filesystem",
                display_path.display()
            )));
        }
        if FileType::from_raw_mode(stat.st_mode) == FileType::Directory {
            let child = fs::openat(parent, name, DIRECTORY_FLAGS, Mode::empty())
                .map_err(|error| io_error("open", display_path, error))?;
            let opened = fs::fstat(&child)
                .map_err(|error| io_error("inspect opened", display_path, error))?;
            require_same_inode(&stat, &opened, display_path)?;
            // Candidate and completed bundle directories may be read-only.
            // Mutation is authorized by the global lock and verified owner;
            // make the opened directory owner-private before walking it.
            fs::fchmod(&child, Mode::RWXU)
                .map_err(|error| io_error("prepare removal of", display_path, error))?;
            remove_tree_contents(&child, display_path)?;
            let current = fs::statat(parent, name, fs::AtFlags::SYMLINK_NOFOLLOW)
                .map_err(|error| io_error("reinspect", display_path, error))?;
            require_same_inode(&opened, &current, display_path)?;
            fs::unlinkat(parent, name, fs::AtFlags::REMOVEDIR)
                .map_err(|error| io_error("remove directory", display_path, error))
        } else {
            fs::unlinkat(parent, name, fs::AtFlags::empty())
                .map_err(|error| io_error("remove", display_path, error))
        }
    }

    pub(super) fn remove_opened_directory(
        parent: &impl rustix::fd::AsFd,
        name: &OsStr,
        opened: &impl rustix::fd::AsFd,
        display_path: &Path,
    ) -> Result<(), StoreError> {
        let expected =
            fs::fstat(opened).map_err(|error| io_error("inspect opened", display_path, error))?;
        let current = fs::statat(parent, name, fs::AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|error| io_error("inspect", display_path, error))?;
        require_same_inode(&expected, &current, display_path)?;
        if FileType::from_raw_mode(expected.st_mode) != FileType::Directory {
            return Err(StoreError::Integrity(format!(
                "protected entry '{}' is not the opened directory",
                display_path.display()
            )));
        }
        fs::fchmod(opened, Mode::RWXU)
            .map_err(|error| io_error("prepare removal of", display_path, error))?;
        remove_tree_contents(opened, display_path)?;
        let current = fs::statat(parent, name, fs::AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|error| io_error("reinspect", display_path, error))?;
        require_same_inode(&expected, &current, display_path)?;
        fs::unlinkat(parent, name, fs::AtFlags::REMOVEDIR)
            .map_err(|error| io_error("remove directory", display_path, error))
    }

    fn require_same_inode(
        expected: &fs::Stat,
        actual: &fs::Stat,
        display_path: &Path,
    ) -> Result<(), StoreError> {
        if expected.st_dev != actual.st_dev
            || expected.st_ino != actual.st_ino
            || FileType::from_raw_mode(expected.st_mode) != FileType::from_raw_mode(actual.st_mode)
        {
            return Err(StoreError::Integrity(format!(
                "protected entry '{}' changed during descriptor-relative mutation",
                display_path.display()
            )));
        }
        Ok(())
    }

    fn io_error(action: &str, path: &Path, error: Errno) -> StoreError {
        StoreError::Io(format!(
            "failed to {action} protected state '{}': {error}",
            path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn trusted_test_store() -> (tempfile::TempDir, StateStore) {
        use std::os::unix::fs::PermissionsExt;

        let parent = tempfile::tempdir().unwrap();
        std::fs::set_permissions(parent.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let root = parent.path().join("state");
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let spec = StateRootSpec::explicit(
            root,
            StateScope::User,
            rustix::process::geteuid().as_raw(),
            rustix::process::getegid().as_raw(),
        )
        .unwrap();
        let store = StateStore::from_trusted_test_root(spec).unwrap();
        (parent, store)
    }

    #[test]
    fn explicit_state_roots_must_be_absolute() {
        let error =
            StateRootSpec::explicit(PathBuf::from("relative"), StateScope::User, 1, 1).unwrap_err();
        assert_eq!(error.code(), "state-failed");
        assert!(error.to_string().contains("must be absolute"));
    }

    #[test]
    fn retention_is_inclusive_deterministic_and_preserves_protected_records() {
        use time::macros::datetime;

        let now = datetime!(2026-07-21 0:00 UTC);
        let timestamp = datetime!(2026-07-20 0:00 UTC);
        let tied = retained_record_ids(
            vec![
                RetentionEntry {
                    id: "c".to_string(),
                    at: timestamp,
                    protected: false,
                },
                RetentionEntry {
                    id: "a".to_string(),
                    at: timestamp,
                    protected: false,
                },
                RetentionEntry {
                    id: "b".to_string(),
                    at: timestamp,
                    protected: true,
                },
            ],
            now,
            1,
            FAILURE_RETENTION,
        );
        assert!(tied.contains("a"));
        assert!(tied.contains("b"));
        assert!(!tied.contains("c"));

        let aged = retained_record_ids(
            vec![
                RetentionEntry {
                    id: "boundary".to_string(),
                    at: now - FAILURE_RETENTION,
                    protected: false,
                },
                RetentionEntry {
                    id: "expired-protected".to_string(),
                    at: now - FAILURE_RETENTION - time::Duration::milliseconds(1),
                    protected: true,
                },
            ],
            now,
            10,
            FAILURE_RETENTION,
        );
        assert!(aged.contains("boundary"));
        assert!(aged.contains("expired-protected"));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn atomic_write_failures_leave_one_complete_old_or_new_document() {
        use supported::AtomicWriteBoundary;

        let (_parent, store) = trusted_test_store();
        let path = store.path().join("atomic.json");
        let old = serde_json::json!({"value": "old"});
        let new = serde_json::json!({"value": "new"});
        let boundaries = [
            AtomicWriteBoundary::TemporaryCreated,
            AtomicWriteBoundary::DataWritten,
            AtomicWriteBoundary::FileSynced,
            AtomicWriteBoundary::Renamed,
            AtomicWriteBoundary::DirectorySynced,
        ];

        for boundary in boundaries {
            supported::write_json_atomic(
                &store.root_fd,
                "atomic.json",
                &path,
                &store.spec,
                &old,
                1_024,
                false,
            )
            .unwrap();
            let error = supported::write_json_atomic_observed(
                &store.root_fd,
                "atomic.json",
                &path,
                &store.spec,
                &new,
                supported::AtomicWritePolicy {
                    maximum_bytes: 1_024,
                    reject_existing: false,
                },
                |observed| {
                    if observed == boundary {
                        Err(StoreError::Io(format!(
                            "injected failure after {observed:?}"
                        )))
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err();
            assert!(error.to_string().contains("injected failure"));
            let actual: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let expected = match boundary {
                AtomicWriteBoundary::TemporaryCreated
                | AtomicWriteBoundary::DataWritten
                | AtomicWriteBoundary::FileSynced => &old,
                AtomicWriteBoundary::Renamed | AtomicWriteBoundary::DirectorySynced => &new,
            };
            assert_eq!(&actual, expected, "failure after {boundary:?}");
            assert!(std::fs::read_dir(store.path()).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".checksy-tmp-")));
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn immutable_atomic_publish_never_replaces_a_racing_destination() {
        use std::os::unix::fs::PermissionsExt;
        use supported::{AtomicWriteBoundary, AtomicWritePolicy};

        let (_parent, store) = trusted_test_store();
        let path = store.path().join("immutable.json");
        let candidate = serde_json::json!({"value": "candidate"});
        let error = supported::write_json_atomic_observed(
            &store.root_fd,
            "immutable.json",
            &path,
            &store.spec,
            &candidate,
            AtomicWritePolicy {
                maximum_bytes: 1_024,
                reject_existing: true,
            },
            |boundary| {
                if boundary == AtomicWriteBoundary::FileSynced {
                    std::fs::write(&path, b"racing-writer\n").unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                        .unwrap();
                }
                Ok(())
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("already exists"));
        assert_eq!(std::fs::read(&path).unwrap(), b"racing-writer\n");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn lock_free_readers_observe_only_complete_atomic_documents() {
        let (_parent, store) = trusted_test_store();
        let path = store.path().join("snapshot.json");
        let make_value = |sequence: u64| {
            serde_json::json!({
                "sequence": sequence,
                "payload": format!("{sequence:08}-{}", "x".repeat(32 * 1024))
            })
        };
        supported::write_json_atomic(
            &store.root_fd,
            "snapshot.json",
            &path,
            &store.spec,
            &make_value(0),
            64 * 1024,
            false,
        )
        .unwrap();

        let reader_path = path.clone();
        let reader = std::thread::spawn(move || {
            for _ in 0..500 {
                let bytes = std::fs::read(&reader_path).unwrap();
                let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                let sequence = value["sequence"].as_u64().unwrap();
                assert_eq!(
                    value["payload"].as_str().unwrap(),
                    format!("{sequence:08}-{}", "x".repeat(32 * 1024))
                );
            }
        });
        for sequence in 1..=50 {
            supported::write_json_atomic(
                &store.root_fd,
                "snapshot.json",
                &path,
                &store.spec,
                &make_value(sequence),
                64 * 1024,
                false,
            )
            .unwrap();
        }
        reader.join().unwrap();
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn opened_root_lock_contends_without_reopening_the_path() {
        let (_parent, store) = trusted_test_store();
        let first = store.try_lock().unwrap();
        let second = store.try_lock().unwrap_err();
        assert!(matches!(second, StoreError::Held));
        drop(first);
        store.try_lock().unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn production_opener_rejects_world_writable_ancestors() {
        let path = std::env::temp_dir().join(format!(
            "checksy-state-root-rejected-{}",
            std::process::id()
        ));
        let spec = StateRootSpec::explicit(
            path,
            StateScope::User,
            rustix::process::geteuid().as_raw(),
            rustix::process::getegid().as_raw(),
        )
        .unwrap();
        let error = StateStore::open_or_create(spec).unwrap_err();
        assert!(error.to_string().contains("group- or world-writable"));
    }
}
