//! Provider-independent validation for a materialized definition bundle.
//!
//! Validation is deliberately rooted at a directory descriptor. Every path
//! component is opened with `O_NOFOLLOW`, regular files must have one link,
//! and the tree is hashed twice so callers never receive a descriptor whose
//! definition graph was decoded from a different tree snapshot.

use crate::cache::GitRemote;
use crate::state::identity::Hash256;
use std::fmt;
use std::path::Path;

pub(crate) const MAX_BUNDLE_ENTRIES: u64 = 10_000;
pub(crate) const MAX_BUNDLE_FILE_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const MAX_BUNDLE_EXPANDED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub(crate) const MAX_BUNDLE_PATH_BYTES: usize = 4_096;
pub(crate) const MAX_CONFIG_PATH_BYTES: usize = 1_024;

/// A caller-selectable subset of the hard bundle limits.
///
/// Production callers normally use `default`. Smaller values are useful for
/// protected policy and for exercising exact boundary behavior in tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BundleLimits {
    pub(crate) max_entries: u64,
    pub(crate) max_single_file_bytes: u64,
    pub(crate) max_total_file_bytes: u64,
    pub(crate) max_path_bytes: usize,
}

impl Default for BundleLimits {
    fn default() -> Self {
        Self {
            max_entries: MAX_BUNDLE_ENTRIES,
            max_single_file_bytes: MAX_BUNDLE_FILE_BYTES,
            max_total_file_bytes: MAX_BUNDLE_EXPANDED_BYTES,
            max_path_bytes: MAX_BUNDLE_PATH_BYTES,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedPatternGroup {
    /// Normalized bundle-relative path of the configuration that owns this
    /// group. Pattern negations never cross this boundary.
    pub(crate) definition_path: String,
    /// Normalized bundle-relative regular-file paths in deterministic order.
    pub(crate) matches: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedBundle {
    pub(crate) bundle_sha256: Hash256,
    pub(crate) entry_count: u64,
    pub(crate) expanded_bytes: u64,
    /// First-seen depth-first order, beginning with the selected root.
    pub(crate) definitions: Vec<String>,
    /// One group for every visited definition, including empty groups.
    pub(crate) pattern_groups: Vec<ValidatedPatternGroup>,
    /// Exact, first-seen external Git dependencies. No network access occurs.
    pub(crate) git_dependencies: Vec<GitRemote>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IntegrityErrorKind {
    UnsupportedPlatform,
    InvalidPath,
    InvalidEntry,
    LimitExceeded,
    DefinitionInvalid,
    DependencyInvalid,
    ConcurrentMutation,
    Io,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IntegrityError {
    kind: IntegrityErrorKind,
    message: String,
}

impl IntegrityError {
    fn new(kind: IntegrityErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub(crate) fn kind(&self) -> IntegrityErrorKind {
        self.kind
    }
}

impl fmt::Display for IntegrityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for IntegrityError {}

/// Validate and digest a materialized bundle without following the selected
/// root if it is a symbolic link.
pub(crate) fn validate_bundle(
    root: &Path,
    config_path: &str,
    limits: BundleLimits,
) -> Result<ValidatedBundle, IntegrityError> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        supported::validate_path(root, config_path, limits)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (root, config_path, limits);
        Err(IntegrityError::new(
            IntegrityErrorKind::UnsupportedPlatform,
            "bundle integrity validation is supported only on Linux and macOS",
        ))
    }
}

/// Validate a bundle through an already trusted directory descriptor.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn validate_bundle_at(
    root_fd: &impl rustix::fd::AsFd,
    config_path: &str,
    limits: BundleLimits,
) -> Result<ValidatedBundle, IntegrityError> {
    supported::validate_fd(root_fd, config_path, limits)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod supported {
    use super::{
        BundleLimits, Hash256, IntegrityError, IntegrityErrorKind, ValidatedBundle,
        ValidatedPatternGroup, MAX_BUNDLE_ENTRIES, MAX_BUNDLE_EXPANDED_BYTES,
        MAX_BUNDLE_FILE_BYTES, MAX_BUNDLE_PATH_BYTES, MAX_CONFIG_PATH_BYTES,
    };
    use crate::cache::GitRemote;
    use crate::config::parse_git_remote;
    use crate::schema::Config;
    use glob::{MatchOptions, Pattern};
    use rustix::fd::{AsFd, OwnedFd};
    use rustix::fs::{self, FileType, Mode, OFlags};
    use sha2::{Digest, Sha256};
    use std::collections::{BTreeSet, HashMap, HashSet};
    use std::ffi::OsString;
    use std::fs::File;
    use std::io::Read;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::Path;

    const BUNDLE_DOMAIN: &[u8] = b"checksy-bundle-v1\0";
    const TEMP_PREFIX: &str = ".checksy-tmp-";
    const RESERVED_ROOT_NAMES: &[&str] = &[
        "generation.json",
        "state.json",
        "lock",
        "trust",
        "policy.json",
        "staging",
        "failures",
        "audit",
    ];

    const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::NONBLOCK)
        .union(OFlags::CLOEXEC);
    const FILE_FLAGS: OFlags = OFlags::RDONLY
        .union(OFlags::NOFOLLOW)
        .union(OFlags::NONBLOCK)
        .union(OFlags::CLOEXEC);

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum EntryKind {
        Directory,
        File,
    }

    impl EntryKind {
        fn digest_name(self) -> &'static [u8] {
            match self {
                Self::Directory => b"directory",
                Self::File => b"file",
            }
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct TreeEntry {
        path: String,
        kind: EntryKind,
        executable: bool,
        size: u64,
        device: u64,
        inode: u64,
        content_sha256: Option<Hash256>,
    }

    #[derive(Debug, Eq, PartialEq)]
    struct Snapshot {
        entries: Vec<TreeEntry>,
        digest: Hash256,
        total_bytes: u64,
    }

    struct ResolvedGraph {
        definitions: Vec<String>,
        pattern_groups: Vec<ValidatedPatternGroup>,
        git_dependencies: Vec<GitRemote>,
    }

    enum Work {
        Enter(String),
        Git(GitRemote),
        Leave(String),
    }

    pub(super) fn validate_path(
        root: &Path,
        config_path: &str,
        limits: BundleLimits,
    ) -> Result<ValidatedBundle, IntegrityError> {
        let root_fd = fs::openat(fs::cwd(), root, DIRECTORY_FLAGS, Mode::empty()).map_err(
            |error| match error {
                rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR => invalid_entry(format!(
                    "bundle root '{}' must be a real directory, not a link or special file",
                    root.display()
                )),
                other => io_error(format!("open bundle root '{}'", root.display()), other),
            },
        )?;
        validate_fd(&root_fd, config_path, limits)
    }

    pub(super) fn validate_fd(
        root_fd: &impl AsFd,
        config_path: &str,
        limits: BundleLimits,
    ) -> Result<ValidatedBundle, IntegrityError> {
        validate_limits(limits)?;
        let config_path = validate_root_config_path(config_path)?;
        validate_root_descriptor(root_fd)?;

        let first = snapshot(root_fd, limits)?;
        let index = index_entries(&first.entries);
        require_regular_config(&index, &config_path)?;
        let graph = resolve_definitions(root_fd, &index, &config_path, limits)?;

        // The second complete snapshot binds the decoded graph and selected
        // pattern files to the same stable bundle contents returned below.
        let second = snapshot(root_fd, limits)?;
        if first != second {
            return Err(IntegrityError::new(
                IntegrityErrorKind::ConcurrentMutation,
                "bundle changed while it was being validated",
            ));
        }

        Ok(ValidatedBundle {
            bundle_sha256: first.digest,
            entry_count: first.entries.len() as u64,
            expanded_bytes: first.total_bytes,
            definitions: graph.definitions,
            pattern_groups: graph.pattern_groups,
            git_dependencies: graph.git_dependencies,
        })
    }

    fn validate_limits(limits: BundleLimits) -> Result<(), IntegrityError> {
        if limits.max_entries == 0
            || limits.max_single_file_bytes == 0
            || limits.max_total_file_bytes == 0
            || limits.max_path_bytes == 0
        {
            return Err(IntegrityError::new(
                IntegrityErrorKind::LimitExceeded,
                "bundle limits must all be positive",
            ));
        }
        if limits.max_entries > MAX_BUNDLE_ENTRIES
            || limits.max_single_file_bytes > MAX_BUNDLE_FILE_BYTES
            || limits.max_total_file_bytes > MAX_BUNDLE_EXPANDED_BYTES
            || limits.max_path_bytes > MAX_BUNDLE_PATH_BYTES
        {
            return Err(IntegrityError::new(
                IntegrityErrorKind::LimitExceeded,
                "bundle limits cannot exceed the compiled hard maxima",
            ));
        }
        Ok(())
    }

    fn validate_root_descriptor(fd: &impl AsFd) -> Result<(), IntegrityError> {
        let stat = fs::fstat(fd).map_err(|error| io_error("inspect bundle root", error))?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::Directory {
            return Err(IntegrityError::new(
                IntegrityErrorKind::InvalidEntry,
                "bundle root is not a directory",
            ));
        }
        Ok(())
    }

    fn snapshot(root_fd: &impl AsFd, limits: BundleLimits) -> Result<Snapshot, IntegrityError> {
        let mut entries = scan_tree(root_fd, limits)?;
        entries.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));

        let mut digest = Sha256::new();
        digest.update(BUNDLE_DOMAIN);
        for entry in &mut entries {
            hash_field(&mut digest, entry.kind.digest_name());
            hash_field(&mut digest, entry.path.as_bytes());
            hash_field(&mut digest, if entry.executable { b"1" } else { b"0" });
            match entry.kind {
                EntryKind::Directory => hash_field(&mut digest, b""),
                EntryKind::File => {
                    digest.update(entry.size.to_be_bytes());
                    let content_hash =
                        read_regular_file(root_fd, entry, |bytes| digest.update(bytes))?;
                    entry.content_sha256 = Some(content_hash);
                }
            }
        }

        let total_bytes = entries.iter().map(|entry| entry.size).sum();
        Ok(Snapshot {
            entries,
            digest: hash_result(digest),
            total_bytes,
        })
    }

    fn hash_field(hasher: &mut Sha256, bytes: &[u8]) {
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }

    fn hash_result(hasher: Sha256) -> Hash256 {
        let bytes: [u8; 32] = hasher.finalize().into();
        Hash256::from_bytes(bytes)
    }

    fn scan_tree(
        root_fd: &impl AsFd,
        limits: BundleLimits,
    ) -> Result<Vec<TreeEntry>, IntegrityError> {
        let root =
            rustix::io::dup(root_fd).map_err(|error| io_error("duplicate bundle root", error))?;
        let mut stack = vec![(root, String::new())];
        let mut entries = Vec::new();
        let mut seen_paths = HashSet::new();
        let mut total_bytes = 0_u64;

        while let Some((directory_fd, prefix)) = stack.pop() {
            let mut directory = fs::Dir::read_from(&directory_fd)
                .map_err(|error| io_error(display_action("read directory", &prefix), error))?;
            let mut names = Vec::<OsString>::new();
            while let Some(result) = directory.read() {
                let item = result
                    .map_err(|error| io_error(display_action("read directory", &prefix), error))?;
                let name_bytes = item.file_name().to_bytes();
                if name_bytes == b"." || name_bytes == b".." {
                    continue;
                }
                names.push(OsString::from_vec(name_bytes.to_vec()));
            }
            names.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));

            for entry_name in names {
                let name_bytes = entry_name.as_bytes();
                let name = std::str::from_utf8(name_bytes).map_err(|_| {
                    IntegrityError::new(
                        IntegrityErrorKind::InvalidPath,
                        display_action("bundle entry name is not valid UTF-8 under", &prefix),
                    )
                })?;
                validate_entry_component(name, prefix.is_empty())?;
                let path = if prefix.is_empty() {
                    name.to_string()
                } else {
                    format!("{prefix}/{name}")
                };
                if path.len() > limits.max_path_bytes {
                    return Err(limit_error(format!(
                        "bundle path '{}' exceeds {} UTF-8 bytes",
                        path, limits.max_path_bytes
                    )));
                }
                if !seen_paths.insert(path.clone()) {
                    return Err(IntegrityError::new(
                        IntegrityErrorKind::InvalidEntry,
                        format!("bundle contains duplicate path '{path}'"),
                    ));
                }
                let next_count = entries.len() as u64 + 1;
                if next_count > limits.max_entries {
                    return Err(limit_error(format!(
                        "bundle exceeds {} entries",
                        limits.max_entries
                    )));
                }

                let before = fs::statat(&directory_fd, &entry_name, fs::AtFlags::SYMLINK_NOFOLLOW)
                    .map_err(|error| io_error(format!("inspect bundle entry '{path}'"), error))?;
                let file_type = FileType::from_raw_mode(before.st_mode);
                match file_type {
                    FileType::Directory => {
                        let child =
                            fs::openat(&directory_fd, &entry_name, DIRECTORY_FLAGS, Mode::empty())
                                .map_err(|error| {
                                    io_error(format!("open bundle directory '{path}'"), error)
                                })?;
                        let opened = fs::fstat(&child).map_err(|error| {
                            io_error(format!("inspect bundle directory '{path}'"), error)
                        })?;
                        require_same_object(&path, &before, &opened, FileType::Directory)?;
                        entries.push(tree_entry(path.clone(), EntryKind::Directory, &opened, 0));
                        stack.push((child, path));
                    }
                    FileType::RegularFile => {
                        validate_regular_stat(&path, &before)?;
                        let size = stat_size(&path, &before)?;
                        if size > limits.max_single_file_bytes {
                            return Err(limit_error(format!(
                                "bundle file '{}' exceeds {} bytes",
                                path, limits.max_single_file_bytes
                            )));
                        }
                        total_bytes = total_bytes.checked_add(size).ok_or_else(|| {
                            limit_error("bundle expanded byte count overflowed".to_string())
                        })?;
                        if total_bytes > limits.max_total_file_bytes {
                            return Err(limit_error(format!(
                                "bundle exceeds {} expanded file bytes",
                                limits.max_total_file_bytes
                            )));
                        }
                        entries.push(tree_entry(path, EntryKind::File, &before, size));
                    }
                    FileType::Symlink => {
                        return Err(invalid_entry(format!(
                            "bundle entry '{path}' is a symbolic link"
                        )))
                    }
                    FileType::Fifo
                    | FileType::Socket
                    | FileType::CharacterDevice
                    | FileType::BlockDevice
                    | FileType::Unknown => {
                        return Err(invalid_entry(format!(
                            "bundle entry '{path}' is not a regular file or directory"
                        )))
                    }
                }
            }
        }

        Ok(entries)
    }

    fn tree_entry(path: String, kind: EntryKind, stat: &fs::Stat, size: u64) -> TreeEntry {
        TreeEntry {
            path,
            kind,
            executable: kind == EntryKind::File && stat.st_mode & 0o111 != 0,
            size,
            device: stat.st_dev,
            inode: stat.st_ino,
            content_sha256: None,
        }
    }

    fn stat_size(path: &str, stat: &fs::Stat) -> Result<u64, IntegrityError> {
        u64::try_from(stat.st_size)
            .map_err(|_| invalid_entry(format!("bundle file '{path}' has a negative length")))
    }

    fn validate_regular_stat(path: &str, stat: &fs::Stat) -> Result<(), IntegrityError> {
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
            return Err(invalid_entry(format!(
                "bundle entry '{path}' is not a regular file"
            )));
        }
        if stat.st_nlink != 1 {
            return Err(invalid_entry(format!(
                "bundle file '{path}' must have exactly one hard link"
            )));
        }
        Ok(())
    }

    fn require_same_object(
        path: &str,
        expected: &fs::Stat,
        actual: &fs::Stat,
        expected_type: FileType,
    ) -> Result<(), IntegrityError> {
        if FileType::from_raw_mode(actual.st_mode) != expected_type
            || expected.st_dev != actual.st_dev
            || expected.st_ino != actual.st_ino
        {
            return Err(IntegrityError::new(
                IntegrityErrorKind::ConcurrentMutation,
                format!("bundle entry '{path}' changed while it was being opened"),
            ));
        }
        Ok(())
    }

    fn read_regular_file(
        root_fd: &impl AsFd,
        entry: &TreeEntry,
        mut consume: impl FnMut(&[u8]),
    ) -> Result<Hash256, IntegrityError> {
        let fd = open_relative(root_fd, &entry.path, false)?;
        let before = fs::fstat(&fd)
            .map_err(|error| io_error(format!("inspect bundle file '{}'", entry.path), error))?;
        validate_regular_stat(&entry.path, &before)?;
        if before.st_dev != entry.device
            || before.st_ino != entry.inode
            || stat_size(&entry.path, &before)? != entry.size
            || (before.st_mode & 0o111 != 0) != entry.executable
        {
            return Err(IntegrityError::new(
                IntegrityErrorKind::ConcurrentMutation,
                format!("bundle file '{}' changed before it was read", entry.path),
            ));
        }

        let mut file = File::from(fd);
        let mut remaining = entry.size;
        let mut content_hash = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        while remaining != 0 {
            let wanted = remaining.min(buffer.len() as u64) as usize;
            let count = file.read(&mut buffer[..wanted]).map_err(|error| {
                IntegrityError::new(
                    IntegrityErrorKind::Io,
                    format!("failed to read bundle file '{}': {error}", entry.path),
                )
            })?;
            if count == 0 {
                return Err(IntegrityError::new(
                    IntegrityErrorKind::ConcurrentMutation,
                    format!("bundle file '{}' became shorter while read", entry.path),
                ));
            }
            content_hash.update(&buffer[..count]);
            consume(&buffer[..count]);
            remaining -= count as u64;
        }
        let mut extra = [0_u8; 1];
        if file.read(&mut extra).map_err(|error| {
            IntegrityError::new(
                IntegrityErrorKind::Io,
                format!(
                    "failed to finish reading bundle file '{}': {error}",
                    entry.path
                ),
            )
        })? != 0
        {
            return Err(IntegrityError::new(
                IntegrityErrorKind::ConcurrentMutation,
                format!("bundle file '{}' grew while read", entry.path),
            ));
        }
        let after = fs::fstat(&file)
            .map_err(|error| io_error(format!("inspect bundle file '{}'", entry.path), error))?;
        if before.st_dev != after.st_dev
            || before.st_ino != after.st_ino
            || before.st_size != after.st_size
            || (before.st_mode & 0o111) != (after.st_mode & 0o111)
        {
            return Err(IntegrityError::new(
                IntegrityErrorKind::ConcurrentMutation,
                format!("bundle file '{}' changed while read", entry.path),
            ));
        }
        Ok(hash_result(content_hash))
    }

    fn open_relative(
        root_fd: &impl AsFd,
        normalized_path: &str,
        directory: bool,
    ) -> Result<OwnedFd, IntegrityError> {
        let components: Vec<_> = normalized_path.split('/').collect();
        let mut current =
            rustix::io::dup(root_fd).map_err(|error| io_error("duplicate bundle root", error))?;
        for component in &components[..components.len() - 1] {
            current = fs::openat(&current, *component, DIRECTORY_FLAGS, Mode::empty()).map_err(
                |error| io_error(format!("open bundle path '{normalized_path}'"), error),
            )?;
        }
        let flags = if directory {
            DIRECTORY_FLAGS
        } else {
            FILE_FLAGS
        };
        fs::openat(
            &current,
            components[components.len() - 1],
            flags,
            Mode::empty(),
        )
        .map_err(|error| io_error(format!("open bundle path '{normalized_path}'"), error))
    }

    fn index_entries(entries: &[TreeEntry]) -> HashMap<&str, &TreeEntry> {
        entries
            .iter()
            .map(|entry| (entry.path.as_str(), entry))
            .collect()
    }

    fn require_regular_config(
        index: &HashMap<&str, &TreeEntry>,
        path: &str,
    ) -> Result<(), IntegrityError> {
        match index.get(path) {
            Some(entry) if entry.kind == EntryKind::File => Ok(()),
            Some(_) => Err(IntegrityError::new(
                IntegrityErrorKind::DefinitionInvalid,
                format!("definition '{path}' is not a regular file"),
            )),
            None => Err(IntegrityError::new(
                IntegrityErrorKind::DefinitionInvalid,
                format!("definition '{path}' is not present in the bundle"),
            )),
        }
    }

    fn resolve_definitions(
        root_fd: &impl AsFd,
        index: &HashMap<&str, &TreeEntry>,
        root_config: &str,
        limits: BundleLimits,
    ) -> Result<ResolvedGraph, IntegrityError> {
        let mut work = vec![Work::Enter(root_config.to_string())];
        let mut active = Vec::<String>::new();
        let mut completed = HashSet::<String>::new();
        let mut definitions = Vec::new();
        let mut pattern_groups = Vec::new();
        let mut git_dependencies = Vec::new();
        let mut git_seen = HashSet::new();

        while let Some(action) = work.pop() {
            match action {
                Work::Enter(path) => {
                    if let Some(position) = active.iter().position(|candidate| candidate == &path) {
                        let mut chain = active[position..].to_vec();
                        chain.push(path);
                        return Err(IntegrityError::new(
                            IntegrityErrorKind::DefinitionInvalid,
                            format!("definition inclusion cycle: {}", chain.join(" -> ")),
                        ));
                    }
                    if completed.contains(&path) {
                        continue;
                    }
                    require_regular_config(index, &path)?;
                    let entry = index[&path.as_str()];
                    let bytes = read_file_for_definition(root_fd, entry)?;
                    let config: Config = serde_yaml::from_slice(&bytes).map_err(|error| {
                        IntegrityError::new(
                            IntegrityErrorKind::DefinitionInvalid,
                            format!("decode definition '{path}': {error}"),
                        )
                    })?;

                    definitions.push(path.clone());
                    pattern_groups.push(validate_pattern_group(&path, &config, index, limits)?);
                    active.push(path.clone());
                    work.push(Work::Leave(path.clone()));

                    let remotes: Vec<_> = config
                        .preconditions
                        .iter()
                        .chain(config.rules.iter())
                        .filter_map(|rule| rule.remote.as_deref())
                        .collect();
                    for remote in remotes.into_iter().rev() {
                        if let Some(git) = parse_git_remote(remote) {
                            validate_git_dependency(&git)?;
                            work.push(Work::Git(git));
                        } else {
                            let nested = normalize_nested_config_path(&path, remote)?;
                            work.push(Work::Enter(nested));
                        }
                    }
                }
                Work::Git(git) => {
                    if git_seen.insert(git.clone()) {
                        git_dependencies.push(git);
                    }
                }
                Work::Leave(path) => {
                    if active.pop().as_deref() != Some(path.as_str()) {
                        return Err(IntegrityError::new(
                            IntegrityErrorKind::ConcurrentMutation,
                            "internal definition traversal order became inconsistent",
                        ));
                    }
                    completed.insert(path);
                }
            }
        }

        Ok(ResolvedGraph {
            definitions,
            pattern_groups,
            git_dependencies,
        })
    }

    fn read_file_for_definition(
        root_fd: &impl AsFd,
        entry: &TreeEntry,
    ) -> Result<Vec<u8>, IntegrityError> {
        let capacity = usize::try_from(entry.size).unwrap_or(0);
        let mut bytes = Vec::with_capacity(capacity);
        let actual = read_regular_file(root_fd, entry, |chunk| bytes.extend_from_slice(chunk))?;
        if entry.content_sha256 != Some(actual) {
            return Err(IntegrityError::new(
                IntegrityErrorKind::ConcurrentMutation,
                format!("definition '{}' changed after bundle hashing", entry.path),
            ));
        }
        Ok(bytes)
    }

    fn validate_pattern_group(
        definition_path: &str,
        config: &Config,
        index: &HashMap<&str, &TreeEntry>,
        limits: BundleLimits,
    ) -> Result<ValidatedPatternGroup, IntegrityError> {
        let base = definition_path
            .rsplit_once('/')
            .map_or("", |(parent, _)| parent);
        let options = MatchOptions {
            case_sensitive: true,
            require_literal_separator: true,
            require_literal_leading_dot: false,
        };
        let mut selected = BTreeSet::new();

        for (position, raw) in config.patterns.iter().enumerate() {
            let raw = raw.trim();
            let (negated, expression) = match raw.strip_prefix('!') {
                Some(rest) => (true, rest.trim()),
                None => (false, raw),
            };
            validate_pattern_path(expression, limits.max_path_bytes).map_err(|error| {
                IntegrityError::new(
                    IntegrityErrorKind::DefinitionInvalid,
                    format!("definition '{definition_path}' patterns[{position}]: {error}"),
                )
            })?;
            let pattern = Pattern::new(expression).map_err(|error| {
                IntegrityError::new(
                    IntegrityErrorKind::DefinitionInvalid,
                    format!(
                        "definition '{definition_path}' patterns[{position}] is invalid: {error}"
                    ),
                )
            })?;
            let matches = index.values().filter_map(|entry| {
                if entry.kind != EntryKind::File {
                    return None;
                }
                let relative = relative_to_base(&entry.path, base)?;
                pattern
                    .matches_with(relative, options)
                    .then_some(entry.path.clone())
            });
            if negated {
                for path in matches {
                    selected.remove(&path);
                }
            } else {
                selected.extend(matches);
            }
        }

        Ok(ValidatedPatternGroup {
            definition_path: definition_path.to_string(),
            matches: selected.into_iter().collect(),
        })
    }

    fn relative_to_base<'a>(path: &'a str, base: &str) -> Option<&'a str> {
        if base.is_empty() {
            return Some(path);
        }
        path.strip_prefix(base)?.strip_prefix('/')
    }

    fn validate_pattern_path(value: &str, maximum: usize) -> Result<(), String> {
        if value.is_empty() {
            return Err("pattern must not be empty".to_string());
        }
        if value.len() > maximum {
            return Err(format!("pattern exceeds {maximum} UTF-8 bytes"));
        }
        if value.starts_with('/') {
            return Err("pattern must be relative to its defining configuration".to_string());
        }
        if value.contains('\\') {
            return Err("pattern cannot contain a backslash".to_string());
        }
        if value.chars().any(char::is_control) {
            return Err("pattern cannot contain a control character".to_string());
        }
        if value.ends_with('/') || value.split('/').any(|part| part.is_empty() || part == ".") {
            return Err("pattern must use canonical path components".to_string());
        }
        if value.split('/').any(|part| part == "..") {
            return Err("pattern cannot contain a parent traversal component".to_string());
        }
        Ok(())
    }

    fn validate_root_config_path(value: &str) -> Result<String, IntegrityError> {
        validate_canonical_relative_path(value, MAX_CONFIG_PATH_BYTES, "root config path")?;
        Ok(value.to_string())
    }

    fn validate_canonical_relative_path(
        value: &str,
        maximum: usize,
        label: &str,
    ) -> Result<(), IntegrityError> {
        if value.is_empty() {
            return Err(invalid_path(format!("{label} must not be empty")));
        }
        if value.len() > maximum {
            return Err(invalid_path(format!(
                "{label} exceeds {maximum} UTF-8 bytes"
            )));
        }
        if value.starts_with('/') {
            return Err(invalid_path(format!("{label} must be relative")));
        }
        if value.contains('\\') {
            return Err(invalid_path(format!("{label} cannot contain a backslash")));
        }
        if value.chars().any(char::is_control) {
            return Err(invalid_path(format!(
                "{label} cannot contain a control character"
            )));
        }
        if value.ends_with('/')
            || value
                .split('/')
                .any(|component| component.is_empty() || component == "." || component == "..")
        {
            return Err(invalid_path(format!(
                "{label} must use canonical relative path components"
            )));
        }
        Ok(())
    }

    fn normalize_nested_config_path(
        defining_config: &str,
        remote: &str,
    ) -> Result<String, IntegrityError> {
        if remote.is_empty() {
            return Err(definition_error(format!(
                "definition '{defining_config}' has an empty file remote"
            )));
        }
        if remote.starts_with('/') {
            return Err(definition_error(format!(
                "definition '{defining_config}' file remote '{remote}' must be relative"
            )));
        }
        if remote.contains('\\') || remote.chars().any(char::is_control) {
            return Err(definition_error(format!(
                "definition '{defining_config}' file remote '{remote}' contains an invalid character"
            )));
        }
        if remote.ends_with('/') || remote.split('/').any(|component| component.is_empty()) {
            return Err(definition_error(format!(
                "definition '{defining_config}' file remote '{remote}' is not canonical"
            )));
        }

        let mut components: Vec<&str> = defining_config.split('/').collect();
        components.pop();
        for component in remote.split('/') {
            match component {
                "." => {
                    return Err(definition_error(format!(
                        "definition '{defining_config}' file remote '{remote}' contains '.'"
                    )))
                }
                ".." => {
                    if components.pop().is_none() {
                        return Err(definition_error(format!(
                            "definition '{defining_config}' file remote '{remote}' escapes the bundle"
                        )));
                    }
                }
                other => components.push(other),
            }
        }
        let normalized = components.join("/");
        validate_canonical_relative_path(&normalized, MAX_CONFIG_PATH_BYTES, "nested config path")
            .map_err(|error| definition_error(error.to_string()))?;
        Ok(normalized)
    }

    fn validate_git_dependency(git: &GitRemote) -> Result<(), IntegrityError> {
        if git.repo.is_empty() || git.repo.len() > 8_192 {
            return Err(dependency_error(
                "Git repository locator must contain 1 to 8192 UTF-8 bytes".to_string(),
            ));
        }
        if git.repo.contains(['?', '#'])
            || git
                .repo
                .chars()
                .any(|character| character.is_control() || character.is_whitespace())
        {
            return Err(dependency_error(format!(
                "Git repository locator '{}' contains a forbidden character",
                git.repo
            )));
        }
        if git.ref_.is_empty()
            || git.ref_.len() > MAX_CONFIG_PATH_BYTES
            || git.ref_.starts_with('/')
            || git.ref_.ends_with('/')
            || git.ref_.contains("//")
            || git.ref_.contains('\\')
            || git
                .ref_
                .chars()
                .any(|character| character.is_control() || character.is_whitespace())
            || git
                .ref_
                .split('/')
                .any(|component| component == "." || component == "..")
        {
            return Err(dependency_error(format!(
                "Git selector '{}' is not a canonical ref",
                git.ref_
            )));
        }
        validate_canonical_relative_path(&git.path, MAX_CONFIG_PATH_BYTES, "Git config path")
            .map_err(|error| dependency_error(error.to_string()))
    }

    fn validate_entry_component(value: &str, at_root: bool) -> Result<(), IntegrityError> {
        if value == ".git" {
            return Err(invalid_entry(
                "bundle cannot contain a '.git' path component".to_string(),
            ));
        }
        if value.contains('\\') || value.chars().any(char::is_control) {
            return Err(invalid_path(format!(
                "bundle path component '{value}' contains a forbidden character"
            )));
        }
        if at_root && (RESERVED_ROOT_NAMES.contains(&value) || value.starts_with(TEMP_PREFIX)) {
            return Err(invalid_entry(format!(
                "bundle root entry '{value}' is reserved for protected state"
            )));
        }
        Ok(())
    }

    fn display_action(action: &str, path: &str) -> String {
        if path.is_empty() {
            format!("{action} at bundle root")
        } else {
            format!("{action} '{path}'")
        }
    }

    fn invalid_path(message: String) -> IntegrityError {
        IntegrityError::new(IntegrityErrorKind::InvalidPath, message)
    }

    fn invalid_entry(message: String) -> IntegrityError {
        IntegrityError::new(IntegrityErrorKind::InvalidEntry, message)
    }

    fn limit_error(message: String) -> IntegrityError {
        IntegrityError::new(IntegrityErrorKind::LimitExceeded, message)
    }

    fn definition_error(message: String) -> IntegrityError {
        IntegrityError::new(IntegrityErrorKind::DefinitionInvalid, message)
    }

    fn dependency_error(message: String) -> IntegrityError {
        IntegrityError::new(IntegrityErrorKind::DependencyInvalid, message)
    }

    fn io_error(action: impl fmt::Display, error: rustix::io::Errno) -> IntegrityError {
        IntegrityError::new(
            IntegrityErrorKind::Io,
            format!("failed to {action}: {error}"),
        )
    }

    use std::fmt;
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::Path;
    use tempfile::TempDir;

    fn write(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    fn validate(root: &Path) -> Result<ValidatedBundle, IntegrityError> {
        validate_bundle(root, ".checksy.yaml", BundleLimits::default())
    }

    fn assert_kind(error: IntegrityError, expected: IntegrityErrorKind) -> String {
        assert_eq!(error.kind(), expected, "unexpected error: {error}");
        error.to_string()
    }

    #[test]
    fn resolves_local_definitions_patterns_and_external_git_in_depth_first_order() {
        let temp = TempDir::new().unwrap();
        write(
            temp.path(),
            ".checksy.yaml",
            br#"
preconditions:
  - remote: defs/child.yaml
  - remote: git+https://example.invalid/repo.git#main:config/checksy.yaml
rules:
  - remote: defs/child.yaml
patterns:
  - scripts/*.sh
  - '!scripts/excluded.sh'
"#,
        );
        write(
            temp.path(),
            "defs/child.yaml",
            br#"
rules:
  - remote: ../shared/grandchild.yaml
patterns:
  - scripts/*.sh
  - '!scripts/excluded.sh'
"#,
        );
        write(temp.path(), "shared/grandchild.yaml", b"rules: []\n");
        write(temp.path(), "scripts/accepted.sh", b"root\n");
        write(temp.path(), "scripts/excluded.sh", b"excluded\n");
        write(temp.path(), "defs/scripts/accepted.sh", b"child\n");
        write(temp.path(), "defs/scripts/excluded.sh", b"excluded\n");

        let result = validate(temp.path()).unwrap();
        assert_eq!(
            result.definitions,
            [".checksy.yaml", "defs/child.yaml", "shared/grandchild.yaml"]
        );
        assert_eq!(result.pattern_groups.len(), 3);
        assert_eq!(result.pattern_groups[0].definition_path, ".checksy.yaml");
        assert_eq!(result.pattern_groups[0].matches, ["scripts/accepted.sh"]);
        assert_eq!(result.pattern_groups[1].definition_path, "defs/child.yaml");
        assert_eq!(
            result.pattern_groups[1].matches,
            ["defs/scripts/accepted.sh"]
        );
        assert!(result.pattern_groups[2].matches.is_empty());
        assert_eq!(
            result.git_dependencies,
            [GitRemote {
                repo: "https://example.invalid/repo.git".to_string(),
                ref_: "main".to_string(),
                path: "config/checksy.yaml".to_string(),
            }]
        );
        assert_eq!(result.entry_count, 11);
    }

    #[test]
    fn digest_is_canonical_and_only_tracks_the_executable_permission_bit() {
        let first = TempDir::new().unwrap();
        fs::create_dir(first.path().join("empty")).unwrap();
        write(first.path(), "script", b"hi\n");
        write(first.path(), ".checksy.yaml", b"{}\n");
        fs::set_permissions(
            first.path().join("script"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();

        let second = TempDir::new().unwrap();
        write(second.path(), ".checksy.yaml", b"{}\n");
        write(second.path(), "script", b"hi\n");
        fs::create_dir(second.path().join("empty")).unwrap();
        fs::set_permissions(
            second.path().join("script"),
            fs::Permissions::from_mode(0o711),
        )
        .unwrap();
        fs::set_permissions(
            second.path().join(".checksy.yaml"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();

        let first_digest = validate(first.path()).unwrap().bundle_sha256;
        let second_digest = validate(second.path()).unwrap().bundle_sha256;
        assert_eq!(first_digest, second_digest);
        assert_eq!(
            first_digest.to_hex(),
            "15216e16556263faca490e3eb3679c912b6d000cab30fb1184c92f4a7af19435"
        );

        fs::set_permissions(
            second.path().join("script"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert_ne!(first_digest, validate(second.path()).unwrap().bundle_sha256);
    }

    #[test]
    fn detects_cycles_and_deduplicates_completed_definitions() {
        let cycle = TempDir::new().unwrap();
        write(
            cycle.path(),
            ".checksy.yaml",
            b"rules:\n  - remote: a.yaml\n",
        );
        write(cycle.path(), "a.yaml", b"rules:\n  - remote: b.yaml\n");
        write(cycle.path(), "b.yaml", b"rules:\n  - remote: a.yaml\n");
        let message = assert_kind(
            validate(cycle.path()).unwrap_err(),
            IntegrityErrorKind::DefinitionInvalid,
        );
        assert!(message.contains("a.yaml -> b.yaml -> a.yaml"), "{message}");

        let repeated = TempDir::new().unwrap();
        write(
            repeated.path(),
            ".checksy.yaml",
            b"preconditions:\n  - remote: child.yaml\nrules:\n  - remote: child.yaml\n",
        );
        write(repeated.path(), "child.yaml", b"rules: []\n");
        assert_eq!(
            validate(repeated.path()).unwrap().definitions,
            [".checksy.yaml", "child.yaml"]
        );
    }

    #[test]
    fn nested_file_remotes_may_normalize_inside_but_never_escape_the_bundle() {
        let allowed = TempDir::new().unwrap();
        write(
            allowed.path(),
            ".checksy.yaml",
            b"rules:\n  - remote: nested/child.yaml\n",
        );
        write(
            allowed.path(),
            "nested/child.yaml",
            b"rules:\n  - remote: ../shared.yaml\n",
        );
        write(allowed.path(), "shared.yaml", b"rules: []\n");
        assert_eq!(
            validate(allowed.path()).unwrap().definitions,
            [".checksy.yaml", "nested/child.yaml", "shared.yaml"]
        );

        for remote in ["../outside.yaml", "/absolute.yaml", "nested\\child.yaml"] {
            let denied = TempDir::new().unwrap();
            write(
                denied.path(),
                ".checksy.yaml",
                format!("rules:\n  - remote: '{remote}'\n"),
            );
            assert_eq!(
                validate(denied.path()).unwrap_err().kind(),
                IntegrityErrorKind::DefinitionInvalid,
                "remote {remote}"
            );
        }
    }

    #[test]
    fn rejects_unsafe_pattern_paths_before_returning_matches() {
        for pattern in ["../outside.sh", "/absolute.sh", "scripts\\*.sh"] {
            let temp = TempDir::new().unwrap();
            write(
                temp.path(),
                ".checksy.yaml",
                format!("patterns:\n  - '{pattern}'\n"),
            );
            write(temp.path(), "scripts/inside.sh", b"inside\n");
            assert_eq!(
                validate(temp.path()).unwrap_err().kind(),
                IntegrityErrorKind::DefinitionInvalid,
                "pattern {pattern}"
            );
        }
    }

    #[test]
    fn rejects_symlinks_hardlinks_and_sockets() {
        let symlinked = TempDir::new().unwrap();
        write(symlinked.path(), ".checksy.yaml", b"{}\n");
        symlink(".checksy.yaml", symlinked.path().join("alias")).unwrap();
        assert_eq!(
            validate(symlinked.path()).unwrap_err().kind(),
            IntegrityErrorKind::InvalidEntry
        );

        let hardlinked = TempDir::new().unwrap();
        write(hardlinked.path(), ".checksy.yaml", b"{}\n");
        fs::hard_link(
            hardlinked.path().join(".checksy.yaml"),
            hardlinked.path().join("alias"),
        )
        .unwrap();
        assert_eq!(
            validate(hardlinked.path()).unwrap_err().kind(),
            IntegrityErrorKind::InvalidEntry
        );

        let socket = TempDir::new().unwrap();
        write(socket.path(), ".checksy.yaml", b"{}\n");
        let _listener =
            std::os::unix::net::UnixListener::bind(socket.path().join("socket")).unwrap();
        assert_eq!(
            validate(socket.path()).unwrap_err().kind(),
            IntegrityErrorKind::InvalidEntry
        );
    }

    #[test]
    fn rejects_git_metadata_reserved_state_names_and_non_utf8_paths() {
        for reserved in [
            "generation.json",
            "state.json",
            "lock",
            "trust",
            "policy.json",
            "staging",
            "failures",
            "audit",
            ".checksy-tmp-1",
        ] {
            let temp = TempDir::new().unwrap();
            write(temp.path(), ".checksy.yaml", b"{}\n");
            write(temp.path(), reserved, b"reserved\n");
            assert_eq!(
                validate(temp.path()).unwrap_err().kind(),
                IntegrityErrorKind::InvalidEntry,
                "reserved {reserved}"
            );
        }

        let git = TempDir::new().unwrap();
        write(git.path(), ".checksy.yaml", b"{}\n");
        write(git.path(), "nested/.git/config", b"metadata\n");
        assert_eq!(
            validate(git.path()).unwrap_err().kind(),
            IntegrityErrorKind::InvalidEntry
        );

        let non_utf8 = TempDir::new().unwrap();
        write(non_utf8.path(), ".checksy.yaml", b"{}\n");
        let name = std::ffi::OsString::from_vec(vec![b'b', 0xff]);
        fs::write(non_utf8.path().join(name), b"invalid\n").unwrap();
        assert_eq!(
            validate(non_utf8.path()).unwrap_err().kind(),
            IntegrityErrorKind::InvalidPath
        );
    }

    #[test]
    fn enforces_inclusive_entry_file_total_and_path_bounds() {
        let one_entry = TempDir::new().unwrap();
        write(one_entry.path(), "a", b"{}\n");
        let config_len = 3_u64;
        let exact = BundleLimits {
            max_entries: 1,
            max_single_file_bytes: config_len,
            max_total_file_bytes: config_len,
            max_path_bytes: 1,
        };
        let valid = validate_bundle(one_entry.path(), "a", exact).unwrap();
        assert_eq!(valid.entry_count, 1);
        assert_eq!(valid.expanded_bytes, config_len);

        let entry_over = TempDir::new().unwrap();
        write(entry_over.path(), "a", b"{}\n");
        write(entry_over.path(), "b", b"");
        assert_eq!(
            validate_bundle(entry_over.path(), "a", exact)
                .unwrap_err()
                .kind(),
            IntegrityErrorKind::LimitExceeded
        );

        let file_over = BundleLimits {
            max_single_file_bytes: config_len - 1,
            ..exact
        };
        assert_eq!(
            validate_bundle(one_entry.path(), "a", file_over)
                .unwrap_err()
                .kind(),
            IntegrityErrorKind::LimitExceeded
        );

        let total_over = BundleLimits {
            max_total_file_bytes: config_len - 1,
            ..exact
        };
        assert_eq!(
            validate_bundle(one_entry.path(), "a", total_over)
                .unwrap_err()
                .kind(),
            IntegrityErrorKind::LimitExceeded
        );

        let path_over = TempDir::new().unwrap();
        write(path_over.path(), "aa", b"{}\n");
        assert_eq!(
            validate_bundle(path_over.path(), "aa", exact)
                .unwrap_err()
                .kind(),
            IntegrityErrorKind::LimitExceeded
        );
    }

    #[test]
    fn rejects_invalid_root_paths_limits_git_dependencies_and_root_symlinks() {
        let temp = TempDir::new().unwrap();
        write(temp.path(), ".checksy.yaml", b"{}\n");
        for config in ["", "/.checksy.yaml", "../.checksy.yaml", "a//b", "a\\b"] {
            assert_eq!(
                validate_bundle(temp.path(), config, BundleLimits::default())
                    .unwrap_err()
                    .kind(),
                IntegrityErrorKind::InvalidPath,
                "config {config:?}"
            );
        }

        let excessive = BundleLimits {
            max_entries: MAX_BUNDLE_ENTRIES + 1,
            ..BundleLimits::default()
        };
        assert_eq!(
            validate_bundle(temp.path(), ".checksy.yaml", excessive)
                .unwrap_err()
                .kind(),
            IntegrityErrorKind::LimitExceeded
        );
        let zero = BundleLimits {
            max_entries: 0,
            ..BundleLimits::default()
        };
        assert_eq!(
            validate_bundle(temp.path(), ".checksy.yaml", zero)
                .unwrap_err()
                .kind(),
            IntegrityErrorKind::LimitExceeded
        );

        for remote in [
            "git+#main:.checksy.yaml",
            "git+https://example.invalid/repo.git?token=x#main:.checksy.yaml",
            "git+https://example.invalid/repo.git#bad ref:.checksy.yaml",
            "git+https://example.invalid/repo.git#main:../outside.yaml",
        ] {
            let dependency = TempDir::new().unwrap();
            write(
                dependency.path(),
                ".checksy.yaml",
                format!("rules:\n  - remote: '{remote}'\n"),
            );
            assert_eq!(
                validate(dependency.path()).unwrap_err().kind(),
                IntegrityErrorKind::DependencyInvalid,
                "Git dependency {remote}"
            );
        }

        let parent = TempDir::new().unwrap();
        fs::create_dir(parent.path().join("real")).unwrap();
        write(&parent.path().join("real"), ".checksy.yaml", b"{}\n");
        symlink("real", parent.path().join("alias")).unwrap();
        assert!(validate(&parent.path().join("real")).is_ok());
        assert_eq!(
            validate(&parent.path().join("alias")).unwrap_err().kind(),
            IntegrityErrorKind::InvalidEntry
        );
    }

    #[test]
    fn checked_in_bundle_contract_vector_is_stable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/pull-agent-contract/state-store/bundle/basic");
        let validated = validate_bundle(&root, ".checksy.yaml", BundleLimits::default()).unwrap();
        assert_eq!(
            validated.bundle_sha256,
            Hash256::parse("1c0c213c52b5c9f70348fd498134102a870bb938c084b7da140771af7a36ef8d")
                .unwrap()
        );
        assert_eq!(validated.entry_count, 5);
        assert!(validated.git_dependencies.is_empty());
    }
}
