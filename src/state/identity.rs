//! Stable, filesystem-safe identities used by the protected state store.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::fmt;
use std::str::FromStr;

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::path::{Component, Path};

const SOURCE_DOMAIN: &[u8] = b"checksy-source-v1\0";
const GENERATION_DOMAIN: &[u8] = b"checksy-generation-v1\0";

/// A lowercase, 64-character SHA-256 value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Hash256([u8; 32]);

impl Hash256 {
    pub(crate) const BYTE_LEN: usize = 32;
    pub(crate) const HEX_LEN: usize = 64;

    pub(crate) fn from_bytes(bytes: [u8; Self::BYTE_LEN]) -> Self {
        Self(bytes)
    }

    pub(crate) fn digest(bytes: impl AsRef<[u8]>) -> Self {
        let digest = Sha256::digest(bytes.as_ref());
        let mut result = [0_u8; Self::BYTE_LEN];
        result.copy_from_slice(&digest);
        Self(result)
    }

    pub(crate) fn parse(value: &str) -> Result<Self, IdentityError> {
        if value.len() != Self::HEX_LEN {
            return Err(IdentityError::InvalidHash(format!(
                "expected {} lowercase hexadecimal characters, found {}",
                Self::HEX_LEN,
                value.len()
            )));
        }

        let mut bytes = [0_u8; Self::BYTE_LEN];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = decode_lower_hex(pair[0]).ok_or_else(|| {
                IdentityError::InvalidHash(
                    "hash must contain only lowercase hexadecimal characters".to_string(),
                )
            })?;
            let low = decode_lower_hex(pair[1]).ok_or_else(|| {
                IdentityError::InvalidHash(
                    "hash must contain only lowercase hexadecimal characters".to_string(),
                )
            })?;
            bytes[index] = (high << 4) | low;
        }
        Ok(Self(bytes))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; Self::BYTE_LEN] {
        &self.0
    }

    pub(crate) fn to_hex(self) -> String {
        let mut output = String::with_capacity(Self::HEX_LEN);
        for byte in self.0 {
            output.push(encode_lower_hex(byte >> 4));
            output.push(encode_lower_hex(byte & 0x0f));
        }
        output
    }
}

impl fmt::Display for Hash256 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl FromStr for Hash256 {
    type Err = IdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for Hash256 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Hash256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

macro_rules! identity_type {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub(crate) struct $name(Hash256);

        impl $name {
            pub(crate) fn from_hash(hash: Hash256) -> Self {
                Self(hash)
            }

            pub(crate) fn parse(value: &str) -> Result<Self, IdentityError> {
                Hash256::parse(value).map(Self)
            }

            pub(crate) fn as_hash(&self) -> &Hash256 {
                &self.0
            }

            pub(crate) fn as_bytes(&self) -> &[u8; Hash256::BYTE_LEN] {
                self.0.as_bytes()
            }

            pub(crate) fn to_hex(self) -> String {
                self.0.to_hex()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = IdentityError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }
    };
}

identity_type!(SourceId);
identity_type!(GenerationId);

/// The already-canonical source fields whose stable digest names state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CanonicalSourceIdentity {
    Local {
        canonical_root: Vec<u8>,
        config_path: String,
    },
    Git {
        repository: String,
        selector: String,
        config_path: String,
    },
    Https {
        manifest_url: String,
    },
}

impl CanonicalSourceIdentity {
    /// Builds an identity from fields that the caller has already canonicalized.
    pub(crate) fn local(canonical_root: Vec<u8>, config_path: String) -> Self {
        Self::Local {
            canonical_root,
            config_path,
        }
    }

    /// Canonicalizes an existing local source and derives its confined config path.
    ///
    /// A relative `selected_config` is interpreted relative to `root`. Both the
    /// source directory and selected regular file are physically canonicalized,
    /// so aliases and in-root symlinks identify their final targets. The
    /// canonical config must remain beneath the canonical root.
    #[cfg(unix)]
    pub(crate) fn local_from_filesystem(
        root: &Path,
        selected_config: &Path,
    ) -> Result<Self, IdentityError> {
        let canonical_root = fs::canonicalize(root).map_err(|error| {
            local_filesystem_error("canonicalize local source root", root, error)
        })?;
        let root_metadata = fs::metadata(&canonical_root).map_err(|error| {
            local_filesystem_error(
                "inspect canonical local source root",
                &canonical_root,
                error,
            )
        })?;
        if !root_metadata.is_dir() {
            return Err(IdentityError::InvalidLocalSource(
                "canonical local source root is not a directory".to_string(),
            ));
        }

        let selected_path = if selected_config.is_absolute() {
            selected_config.to_path_buf()
        } else {
            canonical_root.join(selected_config)
        };
        let canonical_config = fs::canonicalize(&selected_path).map_err(|error| {
            local_filesystem_error("canonicalize selected local config", &selected_path, error)
        })?;
        let config_metadata = fs::metadata(&canonical_config).map_err(|error| {
            local_filesystem_error("inspect canonical local config", &canonical_config, error)
        })?;
        if !config_metadata.is_file() {
            return Err(IdentityError::InvalidLocalSource(
                "canonical selected local config is not a regular file".to_string(),
            ));
        }

        let relative_config = canonical_config
            .strip_prefix(&canonical_root)
            .map_err(|_| IdentityError::LocalConfigOutsideRoot)?;
        let config_path = normalize_local_config_path(relative_config)?;
        Ok(Self::local(
            canonical_root.as_os_str().as_bytes().to_vec(),
            config_path,
        ))
    }

    pub(crate) fn git(repository: String, selector: String, config_path: String) -> Self {
        Self::Git {
            repository,
            selector,
            config_path,
        }
    }

    pub(crate) fn https(manifest_url: String) -> Self {
        Self::Https { manifest_url }
    }

    pub(crate) fn source_id(&self) -> SourceId {
        SourceId::derive(self)
    }

    fn fields(&self) -> Vec<&[u8]> {
        match self {
            Self::Local {
                canonical_root,
                config_path,
            } => vec![b"local", canonical_root, config_path.as_bytes()],
            Self::Git {
                repository,
                selector,
                config_path,
            } => vec![
                b"git",
                repository.as_bytes(),
                selector.as_bytes(),
                config_path.as_bytes(),
            ],
            Self::Https { manifest_url } => vec![b"https", manifest_url.as_bytes()],
        }
    }
}

impl SourceId {
    pub(crate) fn derive(identity: &CanonicalSourceIdentity) -> Self {
        Self(hash_fields(SOURCE_DOMAIN, identity.fields()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ObjectFormat {
    Sha1,
    Sha256,
}

impl ObjectFormat {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
        }
    }

    pub(crate) fn for_object_id(value: &str) -> Result<Self, IdentityError> {
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(IdentityError::InvalidObjectId(
                "Git object ID must be lowercase hexadecimal".to_string(),
            ));
        }
        match value.len() {
            40 => Ok(Self::Sha1),
            64 => Ok(Self::Sha256),
            length => Err(IdentityError::InvalidObjectId(format!(
                "Git object ID must contain 40 or 64 characters, found {length}"
            ))),
        }
    }
}

/// Immutable provider material used to derive a generation directory name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GenerationIdentity {
    Local {
        source_id: SourceId,
        bundle_sha256: Hash256,
        config_path: String,
    },
    Git {
        source_id: SourceId,
        object_format: ObjectFormat,
        peeled_commit: String,
        config_path: String,
    },
    Https {
        source_id: SourceId,
        manifest_sha256: Hash256,
        config_path: String,
    },
}

impl GenerationIdentity {
    pub(crate) fn local(source_id: SourceId, bundle_sha256: Hash256, config_path: String) -> Self {
        Self::Local {
            source_id,
            bundle_sha256,
            config_path,
        }
    }

    pub(crate) fn git(
        source_id: SourceId,
        object_format: ObjectFormat,
        peeled_commit: String,
        config_path: String,
    ) -> Self {
        Self::Git {
            source_id,
            object_format,
            peeled_commit,
            config_path,
        }
    }

    pub(crate) fn https(
        source_id: SourceId,
        manifest_sha256: Hash256,
        config_path: String,
    ) -> Self {
        Self::Https {
            source_id,
            manifest_sha256,
            config_path,
        }
    }

    pub(crate) fn generation_id(&self) -> GenerationId {
        GenerationId::derive(self)
    }

    fn fields(&self) -> Vec<Vec<u8>> {
        match self {
            Self::Local {
                source_id,
                bundle_sha256,
                config_path,
            } => vec![
                source_id.to_hex().into_bytes(),
                b"local".to_vec(),
                bundle_sha256.to_hex().into_bytes(),
                config_path.as_bytes().to_vec(),
            ],
            Self::Git {
                source_id,
                object_format,
                peeled_commit,
                config_path,
            } => vec![
                source_id.to_hex().into_bytes(),
                b"git".to_vec(),
                object_format.as_str().as_bytes().to_vec(),
                peeled_commit.as_bytes().to_vec(),
                config_path.as_bytes().to_vec(),
            ],
            Self::Https {
                source_id,
                manifest_sha256,
                config_path,
            } => vec![
                source_id.to_hex().into_bytes(),
                b"https".to_vec(),
                manifest_sha256.to_hex().into_bytes(),
                config_path.as_bytes().to_vec(),
            ],
        }
    }
}

impl GenerationId {
    pub(crate) fn derive(identity: &GenerationIdentity) -> Self {
        let owned_fields = identity.fields();
        Self(hash_fields(
            GENERATION_DOMAIN,
            owned_fields.iter().map(Vec::as_slice),
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum IdentityError {
    InvalidHash(String),
    InvalidObjectId(String),
    LocalFilesystem(String),
    InvalidLocalSource(String),
    LocalConfigOutsideRoot,
    InvalidLocalConfigPath(String),
    FieldTooLong,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHash(message)
            | Self::InvalidObjectId(message)
            | Self::LocalFilesystem(message)
            | Self::InvalidLocalSource(message)
            | Self::InvalidLocalConfigPath(message) => formatter.write_str(message),
            Self::LocalConfigOutsideRoot => {
                formatter.write_str("canonical selected local config escapes the source root")
            }
            Self::FieldTooLong => formatter.write_str("identity field is too long"),
        }
    }
}

impl std::error::Error for IdentityError {}

#[cfg(unix)]
fn local_filesystem_error(operation: &str, path: &Path, error: std::io::Error) -> IdentityError {
    IdentityError::LocalFilesystem(format!("failed to {operation} {}: {error}", path.display()))
}

#[cfg(unix)]
fn normalize_local_config_path(path: &Path) -> Result<String, IdentityError> {
    const MAX_CONFIG_PATH_BYTES: usize = 1024;

    let mut normalized = String::new();
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(IdentityError::InvalidLocalConfigPath(
                "canonical local config path is not relative and normalized".to_string(),
            ));
        };
        let component = component.to_str().ok_or_else(|| {
            IdentityError::InvalidLocalConfigPath(
                "local config path must contain valid UTF-8 components".to_string(),
            )
        })?;
        if component.is_empty()
            || component == "."
            || component == ".."
            || component
                .chars()
                .any(|character| character == '\\' || character.is_control())
        {
            return Err(IdentityError::InvalidLocalConfigPath(
                "local config path contains a prohibited component".to_string(),
            ));
        }
        if !normalized.is_empty() {
            normalized.push('/');
        }
        normalized.push_str(component);
    }

    if normalized.is_empty() {
        return Err(IdentityError::InvalidLocalConfigPath(
            "local config path cannot be empty".to_string(),
        ));
    }
    if normalized.len() > MAX_CONFIG_PATH_BYTES {
        return Err(IdentityError::InvalidLocalConfigPath(format!(
            "local config path exceeds {MAX_CONFIG_PATH_BYTES} UTF-8 bytes"
        )));
    }
    Ok(normalized)
}

fn hash_fields<'a>(domain: &[u8], fields: impl IntoIterator<Item = &'a [u8]>) -> Hash256 {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for field in fields {
        let length = u64::try_from(field.len()).expect("identity fields fit in a u64");
        hasher.update(length.to_be_bytes());
        hasher.update(field);
    }
    let digest = hasher.finalize();
    let mut bytes = [0_u8; Hash256::BYTE_LEN];
    bytes.copy_from_slice(&digest);
    Hash256::from_bytes(bytes)
}

fn decode_lower_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn encode_lower_hex(nibble: u8) -> char {
    match nibble {
        0..=9 => char::from(b'0' + nibble),
        10..=15 => char::from(b'a' + nibble - 10),
        _ => unreachable!("a nibble is at most 15"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::ffi::OsString;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[test]
    fn hashes_round_trip_as_strict_lowercase_hex() {
        let hash = Hash256::digest(b"checksy");
        let encoded = hash.to_hex();
        assert_eq!(encoded.len(), 64);
        assert_eq!(Hash256::parse(&encoded), Ok(hash));
        assert!(Hash256::parse(&encoded.to_uppercase()).is_err());
        assert!(Hash256::parse("abcd").is_err());

        let json = serde_json::to_string(&hash).unwrap();
        assert_eq!(serde_json::from_str::<Hash256>(&json).unwrap(), hash);
    }

    #[test]
    fn source_ids_are_provider_tagged_and_length_prefixed() {
        let local = CanonicalSourceIdentity::local(
            b"/srv/checksy/source".to_vec(),
            ".checksy.yaml".to_string(),
        );
        let local_again = CanonicalSourceIdentity::local(
            b"/srv/checksy/source".to_vec(),
            ".checksy.yaml".to_string(),
        );
        let https =
            CanonicalSourceIdentity::https("https://agent.example/manifest.json".to_string());
        assert_eq!(local.source_id(), local_again.source_id());
        assert_ne!(local.source_id(), https.source_id());

        let first =
            CanonicalSourceIdentity::git("ab".to_string(), "c".to_string(), "d".to_string());
        let second =
            CanonicalSourceIdentity::git("a".to_string(), "bc".to_string(), "d".to_string());
        assert_ne!(first.source_id(), second.source_id());
    }

    #[cfg(unix)]
    #[test]
    fn local_filesystem_identity_resolves_root_and_config_symlinks() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("source");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("checksy.yaml"), b"rules: []\n").unwrap();

        let root_alias = temporary.path().join("source-alias");
        symlink(&root, &root_alias).unwrap();
        let config_alias = root.join("config-alias.yaml");
        symlink("nested/checksy.yaml", &config_alias).unwrap();

        let through_root_alias = CanonicalSourceIdentity::local_from_filesystem(
            &root_alias,
            Path::new("nested/./checksy.yaml"),
        )
        .unwrap();
        let through_config_alias =
            CanonicalSourceIdentity::local_from_filesystem(&root, Path::new("config-alias.yaml"))
                .unwrap();
        assert_eq!(through_root_alias, through_config_alias);

        let CanonicalSourceIdentity::Local {
            canonical_root,
            config_path,
        } = through_root_alias
        else {
            unreachable!();
        };
        assert_eq!(
            canonical_root,
            fs::canonicalize(&root).unwrap().as_os_str().as_bytes()
        );
        assert_eq!(config_path, "nested/checksy.yaml");
    }

    #[cfg(unix)]
    #[test]
    fn local_filesystem_identity_rejects_config_escape() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("source");
        let outside = temporary.path().join("outside.yaml");
        fs::create_dir(&root).unwrap();
        fs::write(&outside, b"rules: []\n").unwrap();
        symlink(&outside, root.join("escaped.yaml")).unwrap();

        for selected in [root.join("escaped.yaml"), outside.clone()] {
            assert!(matches!(
                CanonicalSourceIdentity::local_from_filesystem(&root, &selected),
                Err(IdentityError::LocalConfigOutsideRoot)
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn local_filesystem_identity_preserves_non_utf8_root_bytes() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary
            .path()
            .join(OsString::from_vec(b"source-\xff".to_vec()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join(".checksy.yaml"), b"rules: []\n").unwrap();

        let identity =
            CanonicalSourceIdentity::local_from_filesystem(&root, Path::new(".checksy.yaml"))
                .unwrap();
        let CanonicalSourceIdentity::Local {
            canonical_root,
            config_path,
        } = identity
        else {
            unreachable!();
        };
        let expected = fs::canonicalize(&root).unwrap();
        assert_eq!(canonical_root, expected.as_os_str().as_bytes());
        assert!(canonical_root.contains(&0xff));
        assert_eq!(config_path, ".checksy.yaml");
    }

    #[test]
    fn contract_identity_vectors_are_stable() {
        let local = CanonicalSourceIdentity::local(
            b"/opt/checksy/source".to_vec(),
            ".checksy.yaml".to_string(),
        );
        assert_eq!(
            local.source_id().to_hex(),
            "7fcb3646319b76245a80ee21cd680b3b1c61ef5866fbd4038a5ef6e1b0b33e2d"
        );

        let mut non_utf8_root = b"/opt/checksy/".to_vec();
        non_utf8_root.push(0xff);
        non_utf8_root.extend_from_slice(b"source");
        let non_utf8 = CanonicalSourceIdentity::local(non_utf8_root, ".checksy.yaml".to_string());
        assert_eq!(
            non_utf8.source_id().to_hex(),
            "ba2ecd11b215f3a394598b9807581cde6bae2aaeac9b8e2dba0844feffc50cce"
        );

        let git = CanonicalSourceIdentity::git(
            "https://example.com/Checks.git".to_string(),
            "refs/heads/main".to_string(),
            "config/checksy.yaml".to_string(),
        );
        assert_eq!(
            git.source_id().to_hex(),
            "15108db35db75551acb26e5e739cda98d6023f33d62e3b3fe88fccad20dad892"
        );

        let https = CanonicalSourceIdentity::https(
            "https://example.com/checksy/manifest.json?channel=stable".to_string(),
        );
        assert_eq!(
            https.source_id().to_hex(),
            "1bc53e89ed2c75f0dcda5d59460f1afc0f0922b4ff963c39e937dbf3bf2dc117"
        );

        assert_eq!(
            hash_fields(SOURCE_DOMAIN, [b"ab".as_slice(), b"c".as_slice()]).to_hex(),
            "60bd2f21a7b527eb4f65da83777d2260f74f99ccc556c54e49ca7d6747baab7a"
        );
        assert_eq!(
            hash_fields(SOURCE_DOMAIN, [b"a".as_slice(), b"bc".as_slice()]).to_hex(),
            "ae6724b88625f7c156a22fa0c5839c29e548e26ea2d02c353fff4e9a453ee6fe"
        );
    }

    #[test]
    fn generation_ids_bind_provider_specific_immutable_material() {
        let source =
            CanonicalSourceIdentity::https("https://agent.example/manifest.json".to_string())
                .source_id();
        let manifest_a = Hash256::digest(b"manifest-a");
        let manifest_b = Hash256::digest(b"manifest-b");
        let first = GenerationIdentity::https(source, manifest_a, ".checksy.yaml".to_string());
        let same = GenerationIdentity::https(source, manifest_a, ".checksy.yaml".to_string());
        let changed = GenerationIdentity::https(source, manifest_b, ".checksy.yaml".to_string());
        assert_eq!(first.generation_id(), same.generation_id());
        assert_ne!(first.generation_id(), changed.generation_id());
    }

    #[test]
    fn git_object_format_is_inferred_without_accepting_uppercase() {
        assert_eq!(
            ObjectFormat::for_object_id(&"a".repeat(40)).unwrap(),
            ObjectFormat::Sha1
        );
        assert_eq!(
            ObjectFormat::for_object_id(&"b".repeat(64)).unwrap(),
            ObjectFormat::Sha256
        );
        assert!(ObjectFormat::for_object_id(&"A".repeat(40)).is_err());
        assert!(ObjectFormat::for_object_id(&"a".repeat(39)).is_err());
    }
}
