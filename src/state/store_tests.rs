use super::identity::{GenerationId, GenerationIdentity, Hash256};
use super::integrity::{validate_bundle, BundleLimits};
use super::model::{
    AuditAction, AuditOutcome, AuditRecord, CanonicalSource, Compliance, ComplianceState,
    FailureRecord, Freshness, Generation, GenerationMarker, MarkerProvider, NativePath,
    NormalizedRelativePath, RequiredNullable, Revision, SafeInteger, Selection, SeverityCounts,
    Signer, SourceKind, StateErrorRecord, StateSeverity, StateSnapshot, StateSource, Success,
    SuccessfulCompliance, Timestamp,
};
use super::store::{LockedStateStore, StateRootSpec, StateScope, StateStore};
use std::fs;
use std::io::{BufRead, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use time::macros::datetime;

fn hash(byte: u8) -> Hash256 {
    Hash256::from_bytes([byte; 32])
}

fn timestamp(second: u8) -> Timestamp {
    Timestamp::parse(&format!("2026-07-21T00:00:{second:02}.000Z")).unwrap()
}

fn local_source() -> StateSource {
    let canonical = CanonicalSource::Local {
        root: NativePath::from_bytes(b"/srv/checksy/source".to_vec()).unwrap(),
        config_path: NormalizedRelativePath::parse(".checksy.yaml").unwrap(),
    };
    let id = canonical.identity().unwrap().source_id();
    StateSource {
        id,
        kind: SourceKind::Local,
        display: "/srv/checksy/source".to_string(),
        canonical,
    }
}

fn initial_snapshot(source: StateSource) -> StateSnapshot {
    StateSnapshot {
        schema_version: SafeInteger::new(1).unwrap(),
        snapshot_sequence: SafeInteger::new(1).unwrap(),
        source,
        selection: Selection::empty(),
        freshness: Freshness::Local {
            snapshot_sha256: RequiredNullable::null(),
        },
        last_attempt: RequiredNullable::null(),
        last_success: RequiredNullable::null(),
        last_error: RequiredNullable::null(),
        recorded_compliance: RequiredNullable::null(),
        updated_at: timestamp(0),
    }
}

struct TestStore {
    _temporary: tempfile::TempDir,
    parent: PathBuf,
    root: PathBuf,
    store: StateStore,
}

impl TestStore {
    fn new(scope: StateScope) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let parent = temporary.path().to_path_buf();
        let root = parent.join("state");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(
            &root,
            fs::Permissions::from_mode(match scope {
                StateScope::User => 0o700,
                StateScope::System => 0o755,
            }),
        )
        .unwrap();
        let spec = StateRootSpec::explicit(
            root.clone(),
            scope,
            rustix::process::geteuid().as_raw(),
            rustix::process::getegid().as_raw(),
        )
        .unwrap();
        let store = StateStore::from_trusted_test_root(spec).unwrap();
        Self {
            _temporary: temporary,
            parent,
            root,
            store,
        }
    }

    fn initialize(&self, snapshot: &StateSnapshot) {
        drop(self.initialize_locked(snapshot));
    }

    fn initialize_locked<'a>(&'a self, snapshot: &StateSnapshot) -> LockedStateStore<'a> {
        let locked = self.store.try_lock().unwrap();
        locked.initialize_source(snapshot).unwrap();
        locked
    }

    fn source_root(&self, source: &StateSource) -> PathBuf {
        self.root.join("sources").join(source.id.to_hex())
    }
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
}

fn write_candidate(
    test: &TestStore,
    source: &StateSource,
    candidate_id: Hash256,
    executable: bool,
) -> PathBuf {
    let candidate = test
        .source_root(source)
        .join("staging")
        .join(candidate_id.to_hex());
    let bundle = candidate.join("bundle");
    fs::create_dir(&candidate).unwrap();
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir(&bundle).unwrap();
    fs::set_permissions(&bundle, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        bundle.join(".checksy.yaml"),
        "rules:\n  - check: \"true\"\n",
    )
    .unwrap();
    fs::write(bundle.join("asset.txt"), "owned by this generation\n").unwrap();
    let script = bundle.join("script.sh");
    fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
    if executable {
        fs::set_permissions(script, fs::Permissions::from_mode(0o755)).unwrap();
    }
    bundle
}

fn local_marker(source: &StateSource, bundle: &Path) -> GenerationMarker {
    let validated = validate_bundle(bundle, ".checksy.yaml", BundleLimits::default()).unwrap();
    let config_path = NormalizedRelativePath::parse(".checksy.yaml").unwrap();
    let generation_id = GenerationIdentity::local(
        source.id,
        validated.bundle_sha256,
        config_path.as_str().to_string(),
    )
    .generation_id();
    GenerationMarker {
        schema_version: SafeInteger::new(1).unwrap(),
        completed: true,
        source_id: source.id,
        generation_id,
        config_path,
        bundle_sha256: validated.bundle_sha256,
        provider: MarkerProvider::Local,
    }
}

fn selected_successor(initial: &StateSnapshot, marker: &GenerationMarker) -> StateSnapshot {
    let revision = Revision::parse("local-revision-1").unwrap();
    let generation = Generation {
        generation_id: marker.generation_id,
        revision: revision.clone(),
        config_path: marker.config_path.clone(),
        provider_generation: RequiredNullable::null(),
        manifest_sha256: RequiredNullable::null(),
        artifact_sha256: RequiredNullable::null(),
        bundle_sha256: marker.bundle_sha256,
        signer: Signer::LocalOperator,
        verified_at: timestamp(1),
        promoted_at: timestamp(2),
    };
    let success = Success {
        at: timestamp(3),
        revision: revision.clone(),
        generation_id: generation.generation_id,
        compliance: SuccessfulCompliance::Compliant,
        offline: false,
        promoted: true,
    };
    let compliance = Compliance {
        state: ComplianceState::Compliant,
        revision,
        generation_id: generation.generation_id,
        checked_at: timestamp(3),
        offline: false,
        fail_severity: StateSeverity::Error,
        failing_rules: SafeInteger::new(0).unwrap(),
        degraded_rules: SafeInteger::new(0).unwrap(),
        counts: SeverityCounts {
            debug: SafeInteger::new(0).unwrap(),
            info: SafeInteger::new(0).unwrap(),
            warn: SafeInteger::new(0).unwrap(),
            error: SafeInteger::new(0).unwrap(),
        },
    };
    StateSnapshot {
        schema_version: SafeInteger::new(1).unwrap(),
        snapshot_sequence: SafeInteger::new(2).unwrap(),
        source: initial.source.clone(),
        selection: Selection {
            current: RequiredNullable::some(generation),
            previous: RequiredNullable::null(),
            additional: Vec::new(),
        },
        freshness: Freshness::Local {
            snapshot_sha256: RequiredNullable::some(marker.bundle_sha256),
        },
        // Publication does not require an attempt on the same snapshot; the
        // current/last-success/compliance relationship remains complete.
        last_attempt: RequiredNullable::null(),
        last_success: RequiredNullable::some(success),
        last_error: RequiredNullable::null(),
        recorded_compliance: RequiredNullable::some(compliance),
        updated_at: timestamp(3),
    }
}

fn assert_initial_layout(scope: StateScope, expected_root_mode: u32) {
    let test = TestStore::new(scope);
    let source = local_source();
    let snapshot = initial_snapshot(source.clone());
    let legacy = test.parent.join(".checksy-cache");
    fs::create_dir(&legacy).unwrap();
    fs::write(legacy.join("sentinel"), "legacy remains\n").unwrap();

    test.initialize(&snapshot);

    let source_root = test.source_root(&source);
    assert_eq!(mode(&test.root), expected_root_mode);
    assert_eq!(mode(&test.root.join("lock")), 0o600);
    assert_eq!(mode(&test.root.join("sources")), expected_root_mode);
    assert_eq!(mode(&source_root), expected_root_mode);
    for name in ["trust", "staging", "failures", "audit"] {
        assert_eq!(mode(&source_root.join(name)), 0o700, "{name}");
    }
    assert_eq!(mode(&source_root.join("generations")), expected_root_mode);
    assert_eq!(mode(&source_root.join("state.json")), 0o600);
    assert!(!source_root.join("policy.json").exists());
    assert!(!source_root.join("current").exists());
    assert!(!source_root.join("previous").exists());
    assert_eq!(
        fs::read_to_string(legacy.join("sentinel")).unwrap(),
        "legacy remains\n"
    );
    assert_eq!(test.store.read_snapshot(source.id).unwrap(), snapshot);
}

#[test]
fn initialization_creates_user_layout_last_without_pointer_files_or_legacy_mutation() {
    assert_initial_layout(StateScope::User, 0o700);
}

#[test]
fn initialization_uses_traversable_system_ancestors_and_private_metadata() {
    assert_initial_layout(StateScope::System, 0o755);
}

#[test]
fn strict_snapshot_reads_fail_closed_and_publication_rejects_stale_sequences() {
    let test = TestStore::new(StateScope::User);
    let source = local_source();
    let first = initial_snapshot(source.clone());
    let locked = test.initialize_locked(&first);

    let mut second = first.clone();
    second.snapshot_sequence = SafeInteger::new(2).unwrap();
    second.updated_at = timestamp(1);
    locked.publish_snapshot(1, &second).unwrap();
    let error = locked.publish_snapshot(1, &second).unwrap_err();
    assert!(error.to_string().contains("stale state update"));
    drop(locked);
    assert_eq!(test.store.read_snapshot(source.id).unwrap(), second);

    let state = test.source_root(&source).join("state.json");
    let mut malformed = vec![0xef, 0xbb, 0xbf];
    malformed.extend(fs::read(&state).unwrap());
    fs::write(&state, malformed).unwrap();
    let error = test.store.read_snapshot(source.id).unwrap_err();
    assert_eq!(error.code(), "state-failed");
    assert!(error.to_string().contains("byte-order mark"));
}

#[test]
fn record_ids_determine_filenames_and_existing_records_are_never_overwritten() {
    let test = TestStore::new(StateScope::User);
    let source = local_source();
    let snapshot = initial_snapshot(source.clone());
    let locked = test.initialize_locked(&snapshot);
    let audit = AuditRecord {
        audit_id: hash(0x31),
        at: timestamp(1),
        actor: "uid:1000".to_string(),
        action: AuditAction::Apply,
        source_id: source.id,
        from_generation_id: RequiredNullable::null(),
        to_generation_id: RequiredNullable::null(),
        from_revision: RequiredNullable::null(),
        to_revision: RequiredNullable::null(),
        reason: RequiredNullable::null(),
        outcome: AuditOutcome::Success,
    };
    let failure = FailureRecord {
        failure_id: hash(0x41),
        attempt_id: hash(0x42),
        at: timestamp(2),
        error: StateErrorRecord {
            code: super::model::ErrorCode::StateFailed,
            phase: super::model::ErrorPhase::State,
            message: "durability failure".to_string(),
            retryable: false,
            partial_mutation_possible: false,
        },
        captured_output: RequiredNullable::null(),
    };

    locked.append_audit_record(source.id, &audit).unwrap();
    locked.append_failure_record(source.id, &failure).unwrap();
    let source_root = test.source_root(&source);
    let audit_path = source_root
        .join("audit")
        .join(format!("{}.json", audit.audit_id));
    let failure_path = source_root
        .join("failures")
        .join(format!("{}.json", failure.failure_id));
    assert_eq!(mode(&audit_path), 0o600);
    assert_eq!(mode(&failure_path), 0o600);
    let before = fs::read(&audit_path).unwrap();
    let error = locked.append_audit_record(source.id, &audit).unwrap_err();
    assert!(error.to_string().contains("already exists"));
    assert_eq!(fs::read(audit_path).unwrap(), before);
}

#[test]
fn sealing_writes_the_marker_last_and_normalizes_bundle_permissions() {
    for (scope, generation_mode, directory_mode, plain_mode, executable_mode) in [
        (StateScope::User, 0o700, 0o500, 0o400, 0o500),
        (StateScope::System, 0o755, 0o555, 0o444, 0o555),
    ] {
        let test = TestStore::new(scope);
        let source = local_source();
        let locked = test.initialize_locked(&initial_snapshot(source.clone()));
        let candidate_id = hash(0x51);
        let bundle = write_candidate(&test, &source, candidate_id, true);
        let marker = local_marker(&source, &bundle);

        let sealed = locked
            .materialize_generation_for_test(&source, candidate_id, &marker)
            .unwrap();
        assert_eq!(sealed.bundle_sha256, marker.bundle_sha256);
        let generation = test
            .source_root(&source)
            .join("generations")
            .join(marker.generation_id.to_hex());
        assert_eq!(mode(&generation), generation_mode);
        assert_eq!(mode(&generation.join("bundle")), directory_mode);
        assert_eq!(mode(&generation.join("bundle/.checksy.yaml")), plain_mode);
        assert_eq!(mode(&generation.join("bundle/asset.txt")), plain_mode);
        assert_eq!(mode(&generation.join("bundle/script.sh")), executable_mode);
        assert_eq!(mode(&generation.join("lease")), 0o600);
        assert_eq!(mode(&generation.join("generation.json")), 0o600);
        assert!(!test.source_root(&source).join("current").exists());
    }
}

#[test]
fn failed_candidate_validation_never_writes_a_completion_marker() {
    let test = TestStore::new(StateScope::User);
    let source = local_source();
    let locked = test.initialize_locked(&initial_snapshot(source.clone()));
    let candidate_id = hash(0x52);
    let bundle = write_candidate(&test, &source, candidate_id, false);
    let marker = local_marker(&source, &bundle);
    fs::write(bundle.join("asset.txt"), "changed after validation\n").unwrap();

    let error = locked
        .seal_staging_candidate(&source, candidate_id, &marker)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("candidate bundle digest mismatch"));
    let candidate = test
        .source_root(&source)
        .join("staging")
        .join(candidate_id.to_hex());
    assert!(!candidate.join("lease").exists());
    assert!(!candidate.join("generation.json").exists());
}

#[test]
fn publication_validates_selected_generation_content_and_marker() {
    let test = TestStore::new(StateScope::User);
    let source = local_source();
    let first = initial_snapshot(source.clone());
    let locked = test.initialize_locked(&first);
    let candidate_id = hash(0x61);
    let bundle = write_candidate(&test, &source, candidate_id, false);
    let marker = local_marker(&source, &bundle);
    locked
        .materialize_generation_for_test(&source, candidate_id, &marker)
        .unwrap();
    let second = selected_successor(&first, &marker);
    locked.publish_snapshot(1, &second).unwrap();
    drop(locked);
    let lease = test
        .store
        .lease_generation(&source, marker.generation_id)
        .unwrap();
    assert_eq!(lease.marker, marker);
    assert_eq!(lease.validated.bundle_sha256, marker.bundle_sha256);
    assert!(lease.bundle_path.ends_with("bundle"));
}

#[test]
fn tampered_selected_content_and_missing_or_mismatched_markers_fail_closed() {
    // Tampering is detected before selection publication.
    let test = TestStore::new(StateScope::User);
    let source = local_source();
    let first = initial_snapshot(source.clone());
    let locked = test.initialize_locked(&first);
    let candidate_id = hash(0x71);
    let bundle = write_candidate(&test, &source, candidate_id, false);
    let marker = local_marker(&source, &bundle);
    locked
        .materialize_generation_for_test(&source, candidate_id, &marker)
        .unwrap();
    let asset = test
        .source_root(&source)
        .join("generations")
        .join(marker.generation_id.to_hex())
        .join("bundle/asset.txt");
    fs::set_permissions(&asset, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&asset, "tampered\n").unwrap();
    fs::set_permissions(&asset, fs::Permissions::from_mode(0o400)).unwrap();
    let error = locked
        .publish_snapshot(1, &selected_successor(&first, &marker))
        .unwrap_err();
    assert!(error.to_string().contains("bundle digest mismatch"));
    drop(locked);

    // A completed-generation directory without its marker is unusable.
    let missing = TestStore::new(StateScope::User);
    let source = local_source();
    let locked = missing.initialize_locked(&initial_snapshot(source.clone()));
    let candidate_id = hash(0x72);
    let bundle = write_candidate(&missing, &source, candidate_id, false);
    let marker = local_marker(&source, &bundle);
    locked
        .materialize_generation_for_test(&source, candidate_id, &marker)
        .unwrap();
    let generation = missing
        .source_root(&source)
        .join("generations")
        .join(marker.generation_id.to_hex());
    fs::remove_file(generation.join("generation.json")).unwrap();
    drop(locked);
    let error = missing
        .store
        .lease_generation(&source, marker.generation_id)
        .unwrap_err();
    assert_eq!(error.code(), "state-failed");

    // A valid marker under a different generation ID directory is rejected.
    let wrong_id = GenerationId::from_hash(hash(0x73));
    fs::rename(
        &generation,
        missing
            .source_root(&source)
            .join("generations")
            .join(wrong_id.to_hex()),
    )
    .unwrap();
    // Restore the marker so directory/marker disagreement is the failing edge.
    let restored_marker = missing
        .source_root(&source)
        .join("generations")
        .join(wrong_id.to_hex())
        .join("generation.json");
    fs::write(
        &restored_marker,
        format!("{}\n", serde_json::to_string(&marker).unwrap()),
    )
    .unwrap();
    fs::set_permissions(&restored_marker, fs::Permissions::from_mode(0o600)).unwrap();
    let error = missing
        .store
        .lease_generation(&source, wrong_id)
        .unwrap_err();
    assert!(error.to_string().contains("does not match marker ID"));
}

#[test]
fn a_shared_generation_lease_defers_gc_until_the_reader_releases_it() {
    let test = TestStore::new(StateScope::User);
    let source = local_source();
    let snapshot = initial_snapshot(source.clone());
    let locked = test.initialize_locked(&snapshot);
    let candidate_id = hash(0x81);
    let bundle = write_candidate(&test, &source, candidate_id, false);
    let marker = local_marker(&source, &bundle);
    locked
        .materialize_generation_for_test(&source, candidate_id, &marker)
        .unwrap();
    let generation = test
        .source_root(&source)
        .join("generations")
        .join(marker.generation_id.to_hex());
    let lease = test
        .store
        .lease_generation(&source, marker.generation_id)
        .unwrap();
    locked
        .garbage_collect(&snapshot, datetime!(2026-07-21 0:00 UTC))
        .unwrap();
    assert!(generation.exists(), "a live lease must defer physical GC");
    drop(lease);
    locked
        .garbage_collect(&snapshot, datetime!(2026-07-21 0:00 UTC))
        .unwrap();
    assert!(
        !generation.exists(),
        "a later GC removes the released generation"
    );
}

#[test]
fn source_path_symlinks_are_rejected_without_following_them() {
    use std::os::unix::fs::symlink;

    let test = TestStore::new(StateScope::User);
    let source = local_source();
    fs::create_dir(test.root.join("sources")).unwrap();
    fs::set_permissions(test.root.join("sources"), fs::Permissions::from_mode(0o700)).unwrap();
    let outside = test.parent.join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, test.root.join("sources").join(source.id.to_hex())).unwrap();
    let error = test.store.try_lock().unwrap_err();
    assert_eq!(error.code(), "state-failed");
    assert!(fs::read_dir(outside).unwrap().next().is_none());
}

#[test]
fn failure_retention_keeps_the_newest_ten_and_rejects_renamed_records() {
    let test = TestStore::new(StateScope::User);
    let source = local_source();
    let snapshot = initial_snapshot(source.clone());
    let locked = test.initialize_locked(&snapshot);
    for index in 0_u8..11 {
        locked
            .append_failure_record(
                source.id,
                &FailureRecord {
                    failure_id: hash(index + 1),
                    attempt_id: hash(index + 32),
                    at: timestamp(index),
                    error: StateErrorRecord {
                        code: super::model::ErrorCode::StateFailed,
                        phase: super::model::ErrorPhase::State,
                        message: format!("failure {index}"),
                        retryable: false,
                        partial_mutation_possible: false,
                    },
                    captured_output: RequiredNullable::null(),
                },
            )
            .unwrap();
    }
    locked
        .garbage_collect(&snapshot, datetime!(2026-07-21 0:01 UTC))
        .unwrap();
    let failures = test.source_root(&source).join("failures");
    assert_eq!(fs::read_dir(&failures).unwrap().count(), 10);
    assert!(!failures.join(format!("{}.json", hash(1))).exists());

    let original = failures.join(format!("{}.json", hash(2)));
    fs::rename(&original, failures.join("renamed.json")).unwrap();
    let error = locked
        .garbage_collect(&snapshot, datetime!(2026-07-21 0:01 UTC))
        .unwrap_err();
    assert!(error.to_string().contains("does not match record ID"));
}

#[test]
fn audit_retention_uses_stable_ties_and_preserves_each_selecting_transition_class() {
    let test = TestStore::new(StateScope::User);
    let source = local_source();
    let initial = initial_snapshot(source.clone());
    let locked = test.initialize_locked(&initial);

    let current_candidate = hash(0xa1);
    let current_bundle = write_candidate(&test, &source, current_candidate, false);
    let current_marker = local_marker(&source, &current_bundle);
    locked
        .materialize_generation_for_test(&source, current_candidate, &current_marker)
        .unwrap();
    let previous_candidate = hash(0xa2);
    let previous_bundle = write_candidate(&test, &source, previous_candidate, true);
    let previous_marker = local_marker(&source, &previous_bundle);
    locked
        .materialize_generation_for_test(&source, previous_candidate, &previous_marker)
        .unwrap();

    let mut selected = selected_successor(&initial, &current_marker);
    let current = selected.selection.current.as_ref().unwrap().clone();
    let mut previous = current.clone();
    previous.generation_id = previous_marker.generation_id;
    previous.revision = Revision::parse("local-revision-previous").unwrap();
    previous.bundle_sha256 = previous_marker.bundle_sha256;
    selected.selection.previous = RequiredNullable::some(previous.clone());
    locked.publish_snapshot(1, &selected).unwrap();

    let tied_at = Timestamp::parse("2026-07-20T00:00:00.000Z").unwrap();
    for id in 1_u8..=101 {
        locked
            .append_audit_record(
                source.id,
                &AuditRecord {
                    audit_id: hash(id),
                    at: tied_at.clone(),
                    actor: "retention-test".to_string(),
                    action: AuditAction::Apply,
                    source_id: source.id,
                    from_generation_id: RequiredNullable::null(),
                    to_generation_id: RequiredNullable::null(),
                    from_revision: RequiredNullable::null(),
                    to_revision: RequiredNullable::null(),
                    reason: RequiredNullable::null(),
                    outcome: AuditOutcome::Failed,
                },
            )
            .unwrap();
    }

    // This one old record is both the newest successful selection of current
    // and the newest rollback, exercising overlap deduplication.
    let current_and_rollback = AuditRecord {
        audit_id: hash(200),
        at: Timestamp::parse("2025-01-02T00:00:00.000Z").unwrap(),
        actor: "retention-test".to_string(),
        action: AuditAction::Rollback,
        source_id: source.id,
        from_generation_id: RequiredNullable::some(previous.generation_id),
        to_generation_id: RequiredNullable::some(current.generation_id),
        from_revision: RequiredNullable::some(previous.revision.clone()),
        to_revision: RequiredNullable::some(current.revision.clone()),
        reason: RequiredNullable::some("restore current".to_string()),
        outcome: AuditOutcome::Success,
    };
    // This protects the newest successful selection of previous.
    let previous_selection = AuditRecord {
        audit_id: hash(201),
        at: Timestamp::parse("2025-01-01T00:00:00.000Z").unwrap(),
        actor: "retention-test".to_string(),
        action: AuditAction::Apply,
        source_id: source.id,
        from_generation_id: RequiredNullable::null(),
        to_generation_id: RequiredNullable::some(previous.generation_id),
        from_revision: RequiredNullable::null(),
        to_revision: RequiredNullable::some(previous.revision.clone()),
        reason: RequiredNullable::null(),
        outcome: AuditOutcome::Success,
    };
    let same_generation_observation = AuditRecord {
        audit_id: hash(202),
        at: Timestamp::parse("2025-02-01T00:00:00.000Z").unwrap(),
        actor: "retention-test".to_string(),
        action: AuditAction::Apply,
        source_id: source.id,
        from_generation_id: RequiredNullable::some(current.generation_id),
        to_generation_id: RequiredNullable::some(current.generation_id),
        from_revision: RequiredNullable::some(current.revision.clone()),
        to_revision: RequiredNullable::some(current.revision.clone()),
        reason: RequiredNullable::null(),
        outcome: AuditOutcome::Success,
    };
    locked
        .append_audit_record(source.id, &current_and_rollback)
        .unwrap();
    locked
        .append_audit_record(source.id, &previous_selection)
        .unwrap();
    locked
        .append_audit_record(source.id, &same_generation_observation)
        .unwrap();

    locked
        .garbage_collect(&selected, datetime!(2026-07-21 0:00 UTC))
        .unwrap();
    let audits = test.source_root(&source).join("audit");
    assert_eq!(fs::read_dir(&audits).unwrap().count(), 102);
    // At an exact timestamp tie, ascending stable ID order retains 1..=100.
    assert!(!audits.join(format!("{}.json", hash(101))).exists());
    assert!(audits.join(format!("{}.json", hash(100))).exists());
    // Both records are older than 90 days but protected; the overlapping
    // current/rollback record is present only once.
    assert!(audits
        .join(format!("{}.json", current_and_rollback.audit_id))
        .exists());
    assert!(audits
        .join(format!("{}.json", previous_selection.audit_id))
        .exists());
    assert!(!audits
        .join(format!("{}.json", same_generation_observation.audit_id))
        .exists());
}

#[test]
fn external_git_dependencies_can_be_sealed_but_not_selected() {
    let test = TestStore::new(StateScope::User);
    let source = local_source();
    let initial = initial_snapshot(source.clone());
    let locked = test.initialize_locked(&initial);
    let candidate_id = hash(0xb1);
    let bundle = write_candidate(&test, &source, candidate_id, false);
    fs::write(
        bundle.join(".checksy.yaml"),
        "rules:\n  - remote: git+https://example.invalid/repo.git#main:checks.yaml\n",
    )
    .unwrap();
    let marker = local_marker(&source, &bundle);
    let sealed = locked
        .materialize_generation_for_test(&source, candidate_id, &marker)
        .unwrap();
    assert_eq!(sealed.git_dependencies.len(), 1);
    let error = locked
        .publish_snapshot(1, &selected_successor(&initial, &marker))
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("unpinned external Git dependencies"));
    assert_eq!(test.store.read_snapshot(source.id).unwrap(), initial);
}

fn materialize_unselected_generation(
    test: &TestStore,
    locked: &LockedStateStore<'_>,
    source: &StateSource,
    candidate_byte: u8,
) -> GenerationMarker {
    let candidate_id = hash(candidate_byte);
    let bundle = write_candidate(test, source, candidate_id, false);
    let marker = local_marker(source, &bundle);
    locked
        .materialize_generation_for_test(source, candidate_id, &marker)
        .unwrap();
    marker
}

fn spawn_generation_lease_helper(
    test: &TestStore,
    source: &StateSource,
    generation_id: GenerationId,
    mode: &str,
) -> Child {
    Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "state::store_tests::generation_lease_subprocess_helper",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("CHECKSY_LEASE_HELPER", mode)
        .env("CHECKSY_LEASE_ROOT", &test.root)
        .env(
            "CHECKSY_LEASE_SOURCE",
            serde_json::to_string(source).unwrap(),
        )
        .env("CHECKSY_LEASE_GENERATION", generation_id.to_hex())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn wait_for_helper_line(child: &mut Child, expected: &'static str) {
    let stderr = child.stderr.take().unwrap();
    let (sender, receiver) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let mut lines = std::io::BufReader::new(stderr).lines();
        let result = lines
            .find_map(|line| match line {
                Ok(line) if line.contains(expected) => Some(Ok(())),
                Ok(_) => None,
                Err(error) => Some(Err(error.to_string())),
            })
            .unwrap_or_else(|| Err(format!("helper exited before reporting {expected}")));
        let _ = sender.send(result);
    });
    match receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => reader.join().unwrap(),
        Ok(Err(message)) => {
            let _ = child.kill();
            let _ = child.wait();
            reader.join().unwrap();
            panic!("{message}");
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            reader.join().unwrap();
            panic!("timed out waiting for generation-lease helper: {error}");
        }
    }
}

fn terminate_helper(child: &mut Child) {
    let _ = child.kill();
    let status = child.wait().unwrap();
    assert!(
        !status.success(),
        "lease helper should be killed by its parent"
    );
}

#[test]
fn cross_process_shared_lease_defers_gc_until_process_death() {
    let test = TestStore::new(StateScope::User);
    let source = local_source();
    let snapshot = initial_snapshot(source.clone());
    let locked = test.initialize_locked(&snapshot);
    let marker = materialize_unselected_generation(&test, &locked, &source, 0x91);
    drop(locked);
    let generation = test
        .source_root(&source)
        .join("generations")
        .join(marker.generation_id.to_hex());

    let mut child = spawn_generation_lease_helper(&test, &source, marker.generation_id, "hold");
    wait_for_helper_line(&mut child, "READY");
    let locked = test.store.try_lock().unwrap();
    locked
        .garbage_collect(&snapshot, datetime!(2026-07-21 0:00 UTC))
        .unwrap();
    assert!(generation.exists());

    terminate_helper(&mut child);
    locked
        .garbage_collect(&snapshot, datetime!(2026-07-21 0:00 UTC))
        .unwrap();
    assert!(!generation.exists());
}

#[test]
fn generation_descriptors_are_close_on_exec() {
    let test = TestStore::new(StateScope::User);
    let source = local_source();
    let snapshot = initial_snapshot(source.clone());
    let locked = test.initialize_locked(&snapshot);
    let marker = materialize_unselected_generation(&test, &locked, &source, 0x92);
    drop(locked);
    let generation = test
        .source_root(&source)
        .join("generations")
        .join(marker.generation_id.to_hex());

    let mut child = spawn_generation_lease_helper(&test, &source, marker.generation_id, "exec");
    wait_for_helper_line(&mut child, "EXECED");
    let locked = test.store.try_lock().unwrap();
    locked
        .garbage_collect(&snapshot, datetime!(2026-07-21 0:00 UTC))
        .unwrap();
    assert!(
        !generation.exists(),
        "the exec'd process must not retain any generation lease descriptor"
    );
    terminate_helper(&mut child);
}

#[test]
#[ignore = "isolated helper invoked by the generation lease tests"]
fn generation_lease_subprocess_helper() {
    use std::os::unix::process::CommandExt;

    let Ok(helper_mode) = std::env::var("CHECKSY_LEASE_HELPER") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("CHECKSY_LEASE_ROOT").unwrap());
    let source: StateSource =
        serde_json::from_str(&std::env::var("CHECKSY_LEASE_SOURCE").unwrap()).unwrap();
    let generation_id =
        GenerationId::parse(&std::env::var("CHECKSY_LEASE_GENERATION").unwrap()).unwrap();
    let spec = StateRootSpec::explicit(
        root,
        StateScope::User,
        rustix::process::geteuid().as_raw(),
        rustix::process::getegid().as_raw(),
    )
    .unwrap();
    let store = StateStore::from_trusted_test_root(spec).unwrap();
    let _lease = store.lease_generation(&source, generation_id).unwrap();

    match helper_mode.as_str() {
        "hold" => {
            let mut stderr = std::io::stderr().lock();
            stderr.write_all(b"READY\n").unwrap();
            stderr.flush().unwrap();
            drop(stderr);
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).unwrap();
        }
        "exec" => {
            let error = Command::new("/bin/sh")
                .args(["-c", "printf 'EXECED\\n' >&2; IFS= read -r _"])
                .exec();
            panic!("failed to exec lease probe: {error}");
        }
        other => panic!("unknown lease-helper mode {other}"),
    }
}
