//! Strict persisted-state models for the pull-agent v1 contract.

use super::identity::{
    CanonicalSourceIdentity, GenerationId, GenerationIdentity, Hash256, ObjectFormat, SourceId,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::de::{DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Number, Value};
use std::fmt;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub(crate) const SCHEMA_VERSION: u64 = 1;
pub(crate) const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const MAX_NATIVE_PATH_BYTES: usize = 4_096;
const MAX_CONFIG_PATH_BYTES: usize = 1_024;
const MAX_CAPTURED_STREAM_BYTES: usize = 1_048_576;
const MAX_PERSISTED_STREAM_BYTES: usize = 65_536;
const MAX_ERROR_BYTES: usize = 16_384;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidationError(String);

impl ValidationError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ValidationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DecodeError {
    UnsupportedSchemaVersion(u64),
    Invalid(String),
}

impl DecodeError {
    pub(crate) fn unsupported_schema_version(&self) -> Option<u64> {
        match self {
            Self::UnsupportedSchemaVersion(version) => Some(*version),
            Self::Invalid(_) => None,
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion(version) => {
                write!(formatter, "unsupported state schema version {version}")
            }
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for DecodeError {}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub(crate) struct SafeInteger(u64);

impl SafeInteger {
    pub(crate) fn new(value: u64) -> Result<Self, ValidationError> {
        if value > MAX_SAFE_INTEGER {
            return Err(ValidationError::new(format!(
                "integer {value} exceeds {MAX_SAFE_INTEGER}"
            )));
        }
        Ok(Self(value))
    }

    pub(crate) fn positive(value: u64) -> Result<Self, ValidationError> {
        let value = Self::new(value)?;
        if value.0 == 0 {
            return Err(ValidationError::new("integer must be positive"));
        }
        Ok(value)
    }

    pub(crate) fn get(self) -> u64 {
        self.0
    }

    fn require_positive(self, field: &str) -> Result<(), ValidationError> {
        if self.0 == 0 {
            Err(ValidationError::new(format!("{field} must be positive")))
        } else {
            Ok(())
        }
    }
}

impl<'de> Deserialize<'de> for SafeInteger {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Nullable JSON whose containing property is nevertheless required.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct RequiredNullable<T>(Option<T>);

impl<T> RequiredNullable<T> {
    pub(crate) fn null() -> Self {
        Self(None)
    }

    pub(crate) fn some(value: T) -> Self {
        Self(Some(value))
    }

    pub(crate) fn as_ref(&self) -> Option<&T> {
        self.0.as_ref()
    }

    pub(crate) fn into_option(self) -> Option<T> {
        self.0
    }

    pub(crate) fn is_null(&self) -> bool {
        self.0.is_none()
    }
}

impl<'de, T> Deserialize<'de> for RequiredNullable<T>
where
    T: DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize through a concrete JSON value instead of delegating to
        // `Option<T>`. Serde's missing-field deserializer treats any type that
        // calls `deserialize_option` as absent/null; using `Value` keeps an
        // omitted containing property distinct from an explicit JSON null.
        let value = Value::deserialize(deserializer)?;
        if value.is_null() {
            Ok(Self::null())
        } else {
            serde_json::from_value(value)
                .map(Self::some)
                .map_err(serde::de::Error::custom)
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub(crate) struct Timestamp(String);

impl Timestamp {
    pub(crate) fn parse(value: &str) -> Result<Self, ValidationError> {
        let bytes = value.as_bytes();
        let shape_is_exact = bytes.len() == 24
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes[10] == b'T'
            && bytes[13] == b':'
            && bytes[16] == b':'
            && bytes[19] == b'.'
            && bytes[23] == b'Z'
            && bytes.iter().enumerate().all(|(index, byte)| {
                matches!(index, 4 | 7 | 10 | 13 | 16 | 19 | 23) || byte.is_ascii_digit()
            });
        if !shape_is_exact || OffsetDateTime::parse(value, &Rfc3339).is_err() {
            return Err(ValidationError::new(
                "timestamp must be valid UTC RFC 3339 with exactly millisecond precision",
            ));
        }
        Ok(Self(value.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub(crate) struct NormalizedRelativePath(String);

impl NormalizedRelativePath {
    pub(crate) fn parse(value: &str) -> Result<Self, ValidationError> {
        if value.is_empty() || value.len() > MAX_CONFIG_PATH_BYTES {
            return Err(ValidationError::new(format!(
                "relative path must contain 1 through {MAX_CONFIG_PATH_BYTES} UTF-8 bytes"
            )));
        }
        if value.contains('\\') || value.chars().any(char::is_control) {
            return Err(ValidationError::new(
                "relative path cannot contain a backslash or control character",
            ));
        }
        if value.starts_with('/') || value.ends_with('/') {
            return Err(ValidationError::new(
                "relative path cannot be absolute or end with a slash",
            ));
        }
        if value
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
        {
            return Err(ValidationError::new(
                "relative path cannot contain empty, dot, or parent components",
            ));
        }
        Ok(Self(value.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for NormalizedRelativePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativePath {
    bytes: Vec<u8>,
    display: String,
}

impl NativePath {
    pub(crate) fn from_bytes(bytes: Vec<u8>) -> Result<Self, ValidationError> {
        if bytes.is_empty() || bytes.len() > MAX_NATIVE_PATH_BYTES {
            return Err(ValidationError::new(format!(
                "native path must contain 1 through {MAX_NATIVE_PATH_BYTES} bytes"
            )));
        }
        if bytes[0] != b'/' || bytes.contains(&0) {
            return Err(ValidationError::new(
                "native path must be an absolute Unix path without NUL bytes",
            ));
        }
        if bytes.len() > 1
            && (bytes.last() == Some(&b'/')
                || bytes[1..].split(|byte| *byte == b'/').any(|component| {
                    component.is_empty() || component == b"." || component == b".."
                }))
        {
            return Err(ValidationError::new(
                "native path must be component-normalized",
            ));
        }
        let display = String::from_utf8_lossy(&bytes).into_owned();
        Ok(Self { bytes, display })
    }

    #[cfg(unix)]
    pub(crate) fn from_path(path: &std::path::Path) -> Result<Self, ValidationError> {
        use std::os::unix::ffi::OsStrExt;
        Self::from_bytes(path.as_os_str().as_bytes().to_vec())
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn display(&self) -> &str {
        &self.display
    }

    #[cfg(unix)]
    pub(crate) fn to_path_buf(&self) -> std::path::PathBuf {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(self.bytes.clone()).into()
    }
}

impl Serialize for NativePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("NativePath", 2)?;
        state.serialize_field("bytesBase64Url", &URL_SAFE_NO_PAD.encode(&self.bytes))?;
        state.serialize_field("display", &self.display)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for NativePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct RawNativePath {
            bytes_base64_url: String,
            display: String,
        }

        let raw = RawNativePath::deserialize(deserializer)?;
        if raw.bytes_base64_url.contains('=') {
            return Err(serde::de::Error::custom(
                "native path must use unpadded base64url",
            ));
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(raw.bytes_base64_url.as_bytes())
            .map_err(serde::de::Error::custom)?;
        if URL_SAFE_NO_PAD.encode(&bytes) != raw.bytes_base64_url {
            return Err(serde::de::Error::custom(
                "native path base64url is not canonical",
            ));
        }
        let path = Self::from_bytes(bytes).map_err(serde::de::Error::custom)?;
        if raw.display != path.display {
            return Err(serde::de::Error::custom(
                "native path display does not match its lossless bytes",
            ));
        }
        Ok(path)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct Revision(String);

impl Revision {
    pub(crate) fn parse(value: &str) -> Result<Self, ValidationError> {
        if value.is_empty()
            || value.len() > 256
            || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
            || value
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_whitespace)
            || value.as_bytes().last().is_some_and(u8::is_ascii_whitespace)
        {
            return Err(ValidationError::new(
                "revision must be 1 through 256 printable ASCII bytes without surrounding whitespace",
            ));
        }
        Ok(Self(value.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Revision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct GitObjectId(String);

impl GitObjectId {
    pub(crate) fn parse(value: &str) -> Result<Self, ValidationError> {
        ObjectFormat::for_object_id(value)
            .map_err(|error| ValidationError::new(error.to_string()))?;
        Ok(Self(value.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn object_format(&self) -> ObjectFormat {
        ObjectFormat::for_object_id(&self.0).expect("GitObjectId validates its object format")
    }
}

impl<'de> Deserialize<'de> for GitObjectId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct GitSelector(String);

impl GitSelector {
    pub(crate) fn parse(value: &str) -> Result<Self, ValidationError> {
        let valid_ref = value
            .strip_prefix("refs/heads/")
            .or_else(|| value.strip_prefix("refs/tags/"))
            .is_some_and(|name| {
                !name.is_empty()
                    && value
                        .bytes()
                        .all(|byte| !byte.is_ascii_control() && byte != b' ')
            });
        let valid_oid = value
            .strip_prefix("oid:")
            .is_some_and(|object_id| GitObjectId::parse(object_id).is_ok());
        if value.len() > 1_024 || (!valid_ref && !valid_oid) {
            return Err(ValidationError::new(
                "Git selector must be a full heads/tags ref or oid:<full-object-id>",
            ));
        }
        Ok(Self(value.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for GitSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SourceKind {
    Local,
    Git,
    Https,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum CanonicalSource {
    Local {
        root: NativePath,
        #[serde(rename = "configPath")]
        config_path: NormalizedRelativePath,
    },
    Git {
        repository: String,
        selector: GitSelector,
        #[serde(rename = "configPath")]
        config_path: NormalizedRelativePath,
    },
    Https {
        #[serde(rename = "manifestUrl")]
        manifest_url: String,
    },
}

impl CanonicalSource {
    pub(crate) fn kind(&self) -> SourceKind {
        match self {
            Self::Local { .. } => SourceKind::Local,
            Self::Git { .. } => SourceKind::Git,
            Self::Https { .. } => SourceKind::Https,
        }
    }

    pub(crate) fn identity(&self) -> Result<CanonicalSourceIdentity, ValidationError> {
        match self {
            Self::Local { root, config_path } => Ok(CanonicalSourceIdentity::local(
                root.as_bytes().to_vec(),
                config_path.as_str().to_string(),
            )),
            Self::Git {
                repository,
                selector,
                config_path,
            } => {
                validate_bounded_text(repository, "Git repository", 1, 8_192, true)?;
                Ok(CanonicalSourceIdentity::git(
                    repository.clone(),
                    selector.as_str().to_string(),
                    config_path.as_str().to_string(),
                ))
            }
            Self::Https { manifest_url } => {
                validate_manifest_url(manifest_url)?;
                Ok(CanonicalSourceIdentity::https(manifest_url.clone()))
            }
        }
    }

    pub(crate) fn config_path(&self) -> Option<&NormalizedRelativePath> {
        match self {
            Self::Local { config_path, .. } | Self::Git { config_path, .. } => Some(config_path),
            Self::Https { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StateSource {
    pub(crate) id: SourceId,
    pub(crate) kind: SourceKind,
    pub(crate) display: String,
    pub(crate) canonical: CanonicalSource,
}

impl StateSource {
    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        validate_bounded_text(&self.display, "source display", 1, 8_192, false)?;
        if self.kind != self.canonical.kind() {
            return Err(ValidationError::new(
                "source kind does not match canonical source kind",
            ));
        }
        let expected = self.canonical.identity()?.source_id();
        if self.id != expected {
            return Err(ValidationError::new(format!(
                "source ID mismatch: expected {expected}, found {}",
                self.id
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum MarkerProvider {
    Local,
    Git {
        #[serde(rename = "objectFormat")]
        object_format: ObjectFormat,
        #[serde(rename = "peeledCommit")]
        peeled_commit: GitObjectId,
    },
    Https {
        generation: SafeInteger,
        revision: Revision,
        #[serde(rename = "manifestSha256")]
        manifest_sha256: Hash256,
        #[serde(rename = "artifactSha256")]
        artifact_sha256: Hash256,
    },
}

impl MarkerProvider {
    pub(crate) fn kind(&self) -> SourceKind {
        match self {
            Self::Local => SourceKind::Local,
            Self::Git { .. } => SourceKind::Git,
            Self::Https { .. } => SourceKind::Https,
        }
    }
}

/// Immutable completion proof stored beside a generation bundle.
///
/// Selection-time state such as signer, verification time, and promotion time
/// deliberately does not live here, allowing retained content to be selected
/// again without mutating its content identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GenerationMarker {
    pub(crate) schema_version: SafeInteger,
    pub(crate) completed: bool,
    pub(crate) source_id: SourceId,
    pub(crate) generation_id: GenerationId,
    pub(crate) config_path: NormalizedRelativePath,
    pub(crate) bundle_sha256: Hash256,
    pub(crate) provider: MarkerProvider,
}

impl GenerationMarker {
    pub(crate) fn decode_strict(bytes: &[u8]) -> Result<Self, DecodeError> {
        let value = strict_json_value(bytes)?;
        reject_unsupported_schema(&value)?;
        let marker: Self = from_strict_value(value)?;
        marker.validate().map_err(validation_decode)?;
        Ok(marker)
    }

    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version.get() != SCHEMA_VERSION {
            return Err(ValidationError::new(format!(
                "generation marker schemaVersion must be {SCHEMA_VERSION}"
            )));
        }
        if !self.completed {
            return Err(ValidationError::new(
                "generation marker must set completed to true",
            ));
        }
        let identity = match &self.provider {
            MarkerProvider::Local => GenerationIdentity::local(
                self.source_id,
                self.bundle_sha256,
                self.config_path.as_str().to_string(),
            ),
            MarkerProvider::Git {
                object_format,
                peeled_commit,
            } => {
                if *object_format != peeled_commit.object_format() {
                    return Err(ValidationError::new(
                        "marker Git objectFormat does not match peeledCommit",
                    ));
                }
                GenerationIdentity::git(
                    self.source_id,
                    *object_format,
                    peeled_commit.as_str().to_string(),
                    self.config_path.as_str().to_string(),
                )
            }
            MarkerProvider::Https {
                generation,
                manifest_sha256,
                ..
            } => {
                generation.require_positive("provider.generation")?;
                GenerationIdentity::https(
                    self.source_id,
                    *manifest_sha256,
                    self.config_path.as_str().to_string(),
                )
            }
        };
        let expected = identity.generation_id();
        if self.generation_id != expected {
            return Err(ValidationError::new(format!(
                "generation ID mismatch: expected {expected}, found {}",
                self.generation_id
            )));
        }
        Ok(())
    }

    pub(crate) fn validate_for_source(&self, source: &StateSource) -> Result<(), ValidationError> {
        self.validate()?;
        source.validate()?;
        if self.source_id != source.id || self.provider.kind() != source.kind {
            return Err(ValidationError::new(
                "generation marker does not belong to the selected source",
            ));
        }
        if let Some(expected) = source.canonical.config_path() {
            if &self.config_path != expected {
                return Err(ValidationError::new(
                    "generation marker configPath does not match the source",
                ));
            }
        }
        Ok(())
    }
}

fn validate_bounded_text(
    value: &str,
    field: &str,
    minimum: usize,
    maximum: usize,
    reject_controls: bool,
) -> Result<(), ValidationError> {
    let length = value.len();
    if length < minimum || length > maximum {
        return Err(ValidationError::new(format!(
            "{field} must contain {minimum} through {maximum} UTF-8 bytes"
        )));
    }
    if reject_controls && value.chars().any(char::is_control) {
        return Err(ValidationError::new(format!(
            "{field} cannot contain control characters"
        )));
    }
    Ok(())
}

fn validate_manifest_url(value: &str) -> Result<(), ValidationError> {
    validate_bounded_text(value, "manifest URL", 8, 8_192, true)?;
    if !(value.starts_with("https://") || value.starts_with("http://")) || value.contains('#') {
        return Err(ValidationError::new(
            "manifest URL must be canonical HTTP(S) without a fragment",
        ));
    }
    let authority = value
        .split_once("//")
        .map(|(_, remainder)| remainder.split(['/', '?']).next().unwrap_or(""))
        .unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return Err(ValidationError::new(
            "manifest URL cannot contain userinfo or an empty authority",
        ));
    }
    Ok(())
}

fn validation_decode(error: ValidationError) -> DecodeError {
    DecodeError::Invalid(error.to_string())
}

fn reject_unsupported_schema(value: &Value) -> Result<(), DecodeError> {
    if let Some(version) = value.get("schemaVersion").and_then(Value::as_u64) {
        if version != SCHEMA_VERSION {
            return Err(DecodeError::UnsupportedSchemaVersion(version));
        }
    }
    Ok(())
}

fn from_strict_value<T: DeserializeOwned>(value: Value) -> Result<T, DecodeError> {
    serde_json::from_value(value).map_err(|error| DecodeError::Invalid(error.to_string()))
}

fn strict_json_value(bytes: &[u8]) -> Result<Value, DecodeError> {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err(DecodeError::Invalid(
            "JSON must not contain a UTF-8 byte-order mark".to_string(),
        ));
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = UniqueJsonValue::deserialize(&mut deserializer)
        .map_err(|error| DecodeError::Invalid(error.to_string()))?
        .0;
    deserializer
        .end()
        .map_err(|error| DecodeError::Invalid(error.to_string()))?;
    if !value.is_object() {
        return Err(DecodeError::Invalid(
            "JSON must contain one top-level object".to_string(),
        ));
    }
    Ok(value)
}

struct UniqueJsonValue(Value);

impl<'de> Deserialize<'de> for UniqueJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueJsonVisitor)
    }
}

struct UniqueJsonVisitor;

impl<'de> Visitor<'de> for UniqueJsonVisitor {
    type Value = UniqueJsonValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value without duplicate object members")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Number(Number::from(value))))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Number(Number::from(value))))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .map(UniqueJsonValue)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::String(value.to_string())))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::String(value)))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Null))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Null))
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        UniqueJsonValue::deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<UniqueJsonValue>()? {
            values.push(value.0);
        }
        Ok(UniqueJsonValue(Value::Array(values)))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate JSON member '{key}'"
                )));
            }
            let value = map.next_value::<UniqueJsonValue>()?;
            values.insert(key, value.0);
        }
        Ok(UniqueJsonValue(Value::Object(values)))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum Signer {
    LocalOperator,
    Minisign {
        #[serde(rename = "keyId")]
        key_id: String,
        #[serde(rename = "publicKeySha256")]
        public_key_sha256: Hash256,
    },
    Ssh {
        principal: String,
        #[serde(rename = "keySha256")]
        key_sha256: Hash256,
        #[serde(rename = "signedObject")]
        signed_object: SignedObject,
    },
    GitContentPin {
        #[serde(rename = "objectId")]
        object_id: GitObjectId,
    },
}

impl Signer {
    fn validate_for(
        &self,
        source_kind: SourceKind,
        revision: &Revision,
    ) -> Result<(), ValidationError> {
        match (source_kind, self) {
            (SourceKind::Local, Self::LocalOperator) => Ok(()),
            (
                SourceKind::Https,
                Self::Minisign {
                    key_id,
                    public_key_sha256: _,
                },
            ) => {
                if key_id.len() != 16
                    || !key_id
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                {
                    return Err(ValidationError::new(
                        "Minisign keyId must be 16 lowercase hexadecimal characters",
                    ));
                }
                Ok(())
            }
            (
                SourceKind::Git,
                Self::Ssh {
                    principal,
                    key_sha256: _,
                    signed_object: _,
                },
            ) => validate_bounded_text(principal, "SSH principal", 1, 256, true),
            (SourceKind::Git, Self::GitContentPin { object_id }) => {
                if object_id.as_str() != revision.as_str() {
                    return Err(ValidationError::new(
                        "Git content-pin signer objectId must match the generation revision",
                    ));
                }
                Ok(())
            }
            _ => Err(ValidationError::new(
                "generation signer is incompatible with its source kind",
            )),
        }
    }

    fn matches_marker_provider(&self, provider: &MarkerProvider) -> bool {
        matches!(
            (self, provider),
            (Self::LocalOperator, MarkerProvider::Local)
                | (Self::Ssh { .. }, MarkerProvider::Git { .. })
                | (Self::GitContentPin { .. }, MarkerProvider::Git { .. })
                | (Self::Minisign { .. }, MarkerProvider::Https { .. })
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SignedObject {
    Commit,
    Tag,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Generation {
    pub(crate) generation_id: GenerationId,
    pub(crate) revision: Revision,
    pub(crate) config_path: NormalizedRelativePath,
    pub(crate) provider_generation: RequiredNullable<SafeInteger>,
    pub(crate) manifest_sha256: RequiredNullable<Hash256>,
    pub(crate) artifact_sha256: RequiredNullable<Hash256>,
    pub(crate) bundle_sha256: Hash256,
    pub(crate) signer: Signer,
    pub(crate) verified_at: Timestamp,
    pub(crate) promoted_at: Timestamp,
}

impl Generation {
    pub(crate) fn validate_for_source(&self, source: &StateSource) -> Result<(), ValidationError> {
        source.validate()?;
        if self.promoted_at < self.verified_at {
            return Err(ValidationError::new(
                "generation promotedAt cannot precede verifiedAt",
            ));
        }
        if let Some(config_path) = source.canonical.config_path() {
            if config_path != &self.config_path {
                return Err(ValidationError::new(
                    "generation configPath does not match the canonical source",
                ));
            }
        }
        self.signer.validate_for(source.kind, &self.revision)?;

        let identity = match source.kind {
            SourceKind::Local => {
                require_null(&self.provider_generation, "providerGeneration", "local")?;
                require_null(&self.manifest_sha256, "manifestSha256", "local")?;
                require_null(&self.artifact_sha256, "artifactSha256", "local")?;
                GenerationIdentity::local(
                    source.id,
                    self.bundle_sha256,
                    self.config_path.as_str().to_string(),
                )
            }
            SourceKind::Git => {
                require_null(&self.provider_generation, "providerGeneration", "Git")?;
                require_null(&self.manifest_sha256, "manifestSha256", "Git")?;
                require_null(&self.artifact_sha256, "artifactSha256", "Git")?;
                let object_id = GitObjectId::parse(self.revision.as_str())?;
                if let CanonicalSource::Git { selector, .. } = &source.canonical {
                    if let Some(pinned) = selector.as_str().strip_prefix("oid:") {
                        if object_id.as_str() != pinned {
                            return Err(ValidationError::new(
                                "Git generation revision does not match its protected full-OID selector",
                            ));
                        }
                    }
                }
                GenerationIdentity::git(
                    source.id,
                    object_id.object_format(),
                    object_id.as_str().to_string(),
                    self.config_path.as_str().to_string(),
                )
            }
            SourceKind::Https => {
                let provider_generation =
                    require_some(&self.provider_generation, "providerGeneration", "HTTPS")?;
                provider_generation.require_positive("providerGeneration")?;
                let manifest_sha256 =
                    require_some(&self.manifest_sha256, "manifestSha256", "HTTPS")?;
                require_some(&self.artifact_sha256, "artifactSha256", "HTTPS")?;
                GenerationIdentity::https(
                    source.id,
                    *manifest_sha256,
                    self.config_path.as_str().to_string(),
                )
            }
        };
        let expected = identity.generation_id();
        if self.generation_id != expected {
            return Err(ValidationError::new(format!(
                "generation ID mismatch: expected {expected}, found {}",
                self.generation_id
            )));
        }
        Ok(())
    }

    pub(crate) fn validate_marker(
        &self,
        source: &StateSource,
        marker: &GenerationMarker,
    ) -> Result<(), ValidationError> {
        self.validate_for_source(source)?;
        marker.validate_for_source(source)?;
        if marker.generation_id != self.generation_id
            || marker.config_path != self.config_path
            || marker.bundle_sha256 != self.bundle_sha256
            || !self.signer.matches_marker_provider(&marker.provider)
        {
            return Err(ValidationError::new(
                "generation marker does not match the selected generation",
            ));
        }
        match &marker.provider {
            MarkerProvider::Local => {}
            MarkerProvider::Git { peeled_commit, .. } => {
                if peeled_commit.as_str() != self.revision.as_str() {
                    return Err(ValidationError::new(
                        "generation marker peeledCommit does not match revision",
                    ));
                }
            }
            MarkerProvider::Https {
                generation,
                revision,
                manifest_sha256,
                artifact_sha256,
            } => {
                if self.provider_generation.as_ref() != Some(generation)
                    || revision != &self.revision
                    || self.manifest_sha256.as_ref() != Some(manifest_sha256)
                    || self.artifact_sha256.as_ref() != Some(artifact_sha256)
                {
                    return Err(ValidationError::new(
                        "HTTPS generation marker provider metadata does not match state",
                    ));
                }
            }
        }
        Ok(())
    }
}

fn require_null<T>(
    value: &RequiredNullable<T>,
    field: &str,
    provider: &str,
) -> Result<(), ValidationError> {
    if value.is_null() {
        Ok(())
    } else {
        Err(ValidationError::new(format!(
            "{field} must be null for {provider} generations"
        )))
    }
}

fn require_some<'a, T>(
    value: &'a RequiredNullable<T>,
    field: &str,
    provider: &str,
) -> Result<&'a T, ValidationError> {
    value.as_ref().ok_or_else(|| {
        ValidationError::new(format!("{field} is required for {provider} generations"))
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Selection {
    pub(crate) current: RequiredNullable<Generation>,
    pub(crate) previous: RequiredNullable<Generation>,
    pub(crate) additional: Vec<Generation>,
}

impl Selection {
    pub(crate) fn empty() -> Self {
        Self {
            current: RequiredNullable::null(),
            previous: RequiredNullable::null(),
            additional: Vec::new(),
        }
    }

    pub(crate) fn validate(&self, source: &StateSource) -> Result<(), ValidationError> {
        if self.additional.len() > 1 {
            return Err(ValidationError::new(
                "selection.additional contains more than one generation",
            ));
        }
        if self.current.is_null() && (!self.previous.is_null() || !self.additional.is_empty()) {
            return Err(ValidationError::new(
                "previous/additional selections require a current generation",
            ));
        }

        let mut ids = Vec::new();
        for generation in self.generations() {
            generation.validate_for_source(source)?;
            if ids.contains(&generation.generation_id) {
                return Err(ValidationError::new(
                    "selection contains a duplicate generation ID",
                ));
            }
            ids.push(generation.generation_id);
        }
        Ok(())
    }

    pub(crate) fn generations(&self) -> impl Iterator<Item = &Generation> {
        self.current
            .as_ref()
            .into_iter()
            .chain(self.previous.as_ref())
            .chain(self.additional.iter())
    }

    pub(crate) fn generation(&self, id: GenerationId) -> Option<&Generation> {
        self.generations()
            .find(|generation| generation.generation_id == id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Freshness {
    Local {
        #[serde(rename = "snapshotSha256")]
        snapshot_sha256: RequiredNullable<Hash256>,
    },
    Git {
        selector: GitSelector,
        #[serde(rename = "acceptedCommit")]
        accepted_commit: RequiredNullable<GitObjectId>,
        #[serde(rename = "acceptedTagObject")]
        accepted_tag_object: RequiredNullable<GitObjectId>,
        #[serde(rename = "acceptedAt")]
        accepted_at: RequiredNullable<Timestamp>,
    },
    Https {
        #[serde(rename = "highWaterGeneration")]
        high_water_generation: RequiredNullable<SafeInteger>,
        #[serde(rename = "manifestSha256")]
        manifest_sha256: RequiredNullable<Hash256>,
        revision: RequiredNullable<Revision>,
        #[serde(rename = "artifactSha256")]
        artifact_sha256: RequiredNullable<Hash256>,
        etag: RequiredNullable<String>,
        #[serde(rename = "lastModified")]
        last_modified: RequiredNullable<String>,
        #[serde(rename = "lastOnlineContact")]
        last_online_contact: RequiredNullable<Timestamp>,
    },
}

impl Freshness {
    pub(crate) fn kind(&self) -> SourceKind {
        match self {
            Self::Local { .. } => SourceKind::Local,
            Self::Git { .. } => SourceKind::Git,
            Self::Https { .. } => SourceKind::Https,
        }
    }

    pub(crate) fn validate_for_source(&self, source: &StateSource) -> Result<(), ValidationError> {
        if self.kind() != source.kind {
            return Err(ValidationError::new(
                "freshness kind does not match source kind",
            ));
        }
        match (self, &source.canonical) {
            (Self::Local { .. }, CanonicalSource::Local { .. }) => Ok(()),
            (
                Self::Git {
                    selector,
                    accepted_commit,
                    accepted_tag_object,
                    accepted_at,
                },
                CanonicalSource::Git {
                    selector: source_selector,
                    ..
                },
            ) => {
                if selector != source_selector {
                    return Err(ValidationError::new(
                        "Git freshness selector does not match canonical source",
                    ));
                }
                let commit_present = accepted_commit.as_ref().is_some();
                let accepted_at_present = accepted_at.as_ref().is_some();
                if commit_present != accepted_at_present
                    || (accepted_tag_object.as_ref().is_some() && !commit_present)
                {
                    return Err(ValidationError::new(
                        "Git accepted commit/tag/timestamp fields are inconsistent",
                    ));
                }
                if let Some(pinned) = source_selector.as_str().strip_prefix("oid:") {
                    if accepted_commit
                        .as_ref()
                        .is_some_and(|commit| commit.as_str() != pinned)
                    {
                        return Err(ValidationError::new(
                            "Git accepted commit does not match its protected full-OID selector",
                        ));
                    }
                }
                Ok(())
            }
            (
                Self::Https {
                    high_water_generation,
                    manifest_sha256,
                    revision,
                    artifact_sha256,
                    etag,
                    last_modified,
                    ..
                },
                CanonicalSource::Https { .. },
            ) => {
                let high_water_present = high_water_generation.as_ref().is_some();
                if high_water_generation
                    .as_ref()
                    .is_some_and(|value| value.get() == 0)
                {
                    return Err(ValidationError::new(
                        "HTTPS highWaterGeneration must be positive",
                    ));
                }
                if [
                    manifest_sha256.as_ref().is_some(),
                    revision.as_ref().is_some(),
                    artifact_sha256.as_ref().is_some(),
                ]
                .into_iter()
                .any(|present| present != high_water_present)
                {
                    return Err(ValidationError::new(
                        "HTTPS high-water identity fields must be all null or all present",
                    ));
                }
                if let Some(value) = etag.as_ref() {
                    validate_bounded_text(value, "ETag", 1, 8_192, true)?;
                }
                if let Some(value) = last_modified.as_ref() {
                    validate_bounded_text(value, "Last-Modified", 1, 256, true)?;
                }
                Ok(())
            }
            _ => Err(ValidationError::new(
                "freshness representation is incompatible with canonical source",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AttemptOutcome {
    Success,
    Degraded,
    Noncompliant,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ErrorCode {
    InvalidArguments,
    InteractiveRequired,
    PermissionDenied,
    UnsupportedPlatform,
    UnsupportedSchemaVersion,
    SourceUnavailable,
    OfflineUnavailable,
    AuthenticationFailed,
    ChecksumMismatch,
    RollbackRejected,
    Equivocation,
    ManifestInvalid,
    ArchiveInvalid,
    DefinitionInvalid,
    PolicyDenied,
    StateFailed,
    OperationTimeout,
    SchedulerFailed,
    ComplianceFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ErrorPhase {
    Invocation,
    Lock,
    Source,
    Authentication,
    Extraction,
    Validation,
    Policy,
    Check,
    Fix,
    State,
    Status,
    Scheduling,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StateErrorRecord {
    pub(crate) code: ErrorCode,
    pub(crate) phase: ErrorPhase,
    pub(crate) message: String,
    pub(crate) retryable: bool,
    pub(crate) partial_mutation_possible: bool,
}

impl StateErrorRecord {
    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        if self.message.len() > MAX_ERROR_BYTES {
            return Err(ValidationError::new(format!(
                "error message exceeds {MAX_ERROR_BYTES} UTF-8 bytes"
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CapturedStream {
    pub(crate) head_base64_url: String,
    pub(crate) tail_base64_url: String,
    pub(crate) original_bytes: SafeInteger,
    pub(crate) truncated: bool,
}

impl CapturedStream {
    fn validate_with_limit(&self, limit: usize) -> Result<(), ValidationError> {
        let head = decode_canonical_base64url(&self.head_base64_url, "captured output head")?;
        let tail = decode_canonical_base64url(&self.tail_base64_url, "captured output tail")?;
        let retained = head.len().checked_add(tail.len()).ok_or_else(|| {
            ValidationError::new("captured output retained byte count overflowed")
        })?;
        let original = self.original_bytes.get();
        if self.truncated {
            let half = limit / 2;
            if head.len() != half || tail.len() != half || original <= limit as u64 {
                return Err(ValidationError::new(format!(
                    "truncated output must retain equal {half}-byte head/tail halves and report a larger original"
                )));
            }
        } else if retained > limit || retained as u64 != original {
            return Err(ValidationError::new(
                "untruncated output must retain exactly its original bytes within the limit",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CapturedOutput {
    pub(crate) stdout: CapturedStream,
    pub(crate) stderr: CapturedStream,
}

impl CapturedOutput {
    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        self.stdout.validate_with_limit(MAX_CAPTURED_STREAM_BYTES)?;
        self.stderr.validate_with_limit(MAX_CAPTURED_STREAM_BYTES)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PersistedCapturedStream {
    pub(crate) head_base64_url: String,
    pub(crate) tail_base64_url: String,
    pub(crate) original_bytes: SafeInteger,
    pub(crate) truncated: bool,
}

impl PersistedCapturedStream {
    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        CapturedStream {
            head_base64_url: self.head_base64_url.clone(),
            tail_base64_url: self.tail_base64_url.clone(),
            original_bytes: self.original_bytes,
            truncated: self.truncated,
        }
        .validate_with_limit(MAX_PERSISTED_STREAM_BYTES)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PersistedCapturedOutput {
    pub(crate) stdout: PersistedCapturedStream,
    pub(crate) stderr: PersistedCapturedStream,
}

impl PersistedCapturedOutput {
    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        self.stdout.validate()?;
        self.stderr.validate()
    }
}

fn decode_canonical_base64url(value: &str, field: &str) -> Result<Vec<u8>, ValidationError> {
    if value.contains('=') {
        return Err(ValidationError::new(format!(
            "{field} must use unpadded base64url"
        )));
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(value.as_bytes())
        .map_err(|error| ValidationError::new(format!("invalid {field}: {error}")))?;
    if URL_SAFE_NO_PAD.encode(&decoded) != value {
        return Err(ValidationError::new(format!(
            "{field} is not canonical base64url"
        )));
    }
    Ok(decoded)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Attempt {
    pub(crate) attempt_id: Hash256,
    pub(crate) started_at: Timestamp,
    pub(crate) finished_at: Timestamp,
    pub(crate) outcome: AttemptOutcome,
    pub(crate) exit_code: u8,
    pub(crate) revision: RequiredNullable<Revision>,
    pub(crate) generation_id: RequiredNullable<GenerationId>,
    pub(crate) offline: bool,
    pub(crate) rollback: bool,
    pub(crate) source_changed: bool,
    pub(crate) promoted: bool,
    pub(crate) error: RequiredNullable<StateErrorRecord>,
    pub(crate) captured_output: RequiredNullable<CapturedOutput>,
}

impl Attempt {
    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        if self.finished_at < self.started_at {
            return Err(ValidationError::new(
                "attempt finishedAt cannot precede startedAt",
            ));
        }
        if let Some(error) = self.error.as_ref() {
            error.validate()?;
        }
        if let Some(output) = self.captured_output.as_ref() {
            output.validate()?;
        }
        match self.outcome {
            AttemptOutcome::Success | AttemptOutcome::Degraded => {
                if self.exit_code != 0
                    || self.error.as_ref().is_some()
                    || self.revision.is_null()
                    || self.generation_id.is_null()
                {
                    return Err(ValidationError::new(
                        "successful/degraded attempts require exit 0, revision, generation ID, and null error",
                    ));
                }
            }
            AttemptOutcome::Noncompliant => {
                if self.exit_code != 3
                    || self.promoted
                    || self.error.as_ref().map(|error| error.code)
                        != Some(ErrorCode::ComplianceFailed)
                {
                    return Err(ValidationError::new(
                        "noncompliant attempts require exit 3, compliance-failed, and no promotion",
                    ));
                }
            }
            AttemptOutcome::Error => {
                if self.exit_code != 2 || self.promoted {
                    return Err(ValidationError::new(
                        "operational-error attempts require exit 2 and no promotion",
                    ));
                }
                let code = self.error.as_ref().map(|error| error.code).ok_or_else(|| {
                    ValidationError::new("operational-error attempt requires an error")
                })?;
                if code == ErrorCode::ComplianceFailed {
                    return Err(ValidationError::new(
                        "operational-error attempt cannot use compliance-failed",
                    ));
                }
            }
        }
        if self.offline && (self.rollback || self.source_changed || self.promoted) {
            return Err(ValidationError::new(
                "offline attempts cannot roll back, change source, or promote",
            ));
        }
        if self.promoted
            && (self.offline
                || !matches!(
                    self.outcome,
                    AttemptOutcome::Success | AttemptOutcome::Degraded
                )
                || self.revision.is_null()
                || self.generation_id.is_null())
        {
            return Err(ValidationError::new(
                "promotion requires an online successful/degraded attempt with revision and generation ID",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SuccessfulCompliance {
    Compliant,
    Degraded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Success {
    pub(crate) at: Timestamp,
    pub(crate) revision: Revision,
    pub(crate) generation_id: GenerationId,
    pub(crate) compliance: SuccessfulCompliance,
    pub(crate) offline: bool,
    pub(crate) promoted: bool,
}

impl Success {
    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        if self.offline && self.promoted {
            return Err(ValidationError::new(
                "offline success cannot report promotion",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum StateSeverity {
    Debug,
    Info,
    Warn,
    Error,
}

impl StateSeverity {
    fn rank(self) -> u8 {
        match self {
            Self::Debug => 0,
            Self::Info => 1,
            Self::Warn => 2,
            Self::Error => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ComplianceState {
    Compliant,
    Degraded,
    Noncompliant,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SeverityCounts {
    pub(crate) debug: SafeInteger,
    pub(crate) info: SafeInteger,
    pub(crate) warn: SafeInteger,
    pub(crate) error: SafeInteger,
}

impl SeverityCounts {
    fn values(&self) -> [(StateSeverity, u64); 4] {
        [
            (StateSeverity::Debug, self.debug.get()),
            (StateSeverity::Info, self.info.get()),
            (StateSeverity::Warn, self.warn.get()),
            (StateSeverity::Error, self.error.get()),
        ]
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Compliance {
    pub(crate) state: ComplianceState,
    pub(crate) revision: Revision,
    pub(crate) generation_id: GenerationId,
    pub(crate) checked_at: Timestamp,
    pub(crate) offline: bool,
    pub(crate) fail_severity: StateSeverity,
    pub(crate) failing_rules: SafeInteger,
    pub(crate) degraded_rules: SafeInteger,
    pub(crate) counts: SeverityCounts,
}

impl Compliance {
    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        let mut failing = 0_u64;
        let mut degraded = 0_u64;
        for (severity, count) in self.counts.values() {
            if severity.rank() >= self.fail_severity.rank() {
                failing = failing
                    .checked_add(count)
                    .ok_or_else(|| ValidationError::new("compliance failure count overflowed"))?;
            } else {
                degraded = degraded
                    .checked_add(count)
                    .ok_or_else(|| ValidationError::new("compliance degraded count overflowed"))?;
            }
        }
        if failing != self.failing_rules.get() || degraded != self.degraded_rules.get() {
            return Err(ValidationError::new(
                "compliance aggregate counts do not match severity counts",
            ));
        }
        let expected = if failing > 0 {
            ComplianceState::Noncompliant
        } else if degraded > 0 {
            ComplianceState::Degraded
        } else {
            ComplianceState::Compliant
        };
        if self.state != expected {
            return Err(ValidationError::new(
                "compliance state does not match aggregate failure counts",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StateSnapshot {
    pub(crate) schema_version: SafeInteger,
    pub(crate) snapshot_sequence: SafeInteger,
    pub(crate) source: StateSource,
    pub(crate) selection: Selection,
    pub(crate) freshness: Freshness,
    pub(crate) last_attempt: RequiredNullable<Attempt>,
    pub(crate) last_success: RequiredNullable<Success>,
    pub(crate) last_error: RequiredNullable<StateErrorRecord>,
    pub(crate) recorded_compliance: RequiredNullable<Compliance>,
    pub(crate) updated_at: Timestamp,
}

impl StateSnapshot {
    pub(crate) fn decode_strict(bytes: &[u8]) -> Result<Self, DecodeError> {
        let value = strict_json_value(bytes)?;
        reject_unsupported_schema(&value)?;
        let snapshot: Self = from_strict_value(value)?;
        snapshot.validate().map_err(validation_decode)?;
        Ok(snapshot)
    }

    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version.get() != SCHEMA_VERSION {
            return Err(ValidationError::new(format!(
                "state schemaVersion must be {SCHEMA_VERSION}"
            )));
        }
        self.snapshot_sequence
            .require_positive("snapshotSequence")?;
        self.source.validate()?;
        self.selection.validate(&self.source)?;
        self.freshness.validate_for_source(&self.source)?;

        if let Some(attempt) = self.last_attempt.as_ref() {
            attempt.validate()?;
        }
        if let Some(error) = self.last_error.as_ref() {
            error.validate()?;
        }
        if let Some(compliance) = self.recorded_compliance.as_ref() {
            compliance.validate()?;
        }
        if let Some(success) = self.last_success.as_ref() {
            success.validate()?;
        }

        match self.last_attempt.as_ref() {
            None => {
                if self.last_error.as_ref().is_some() {
                    return Err(ValidationError::new(
                        "lastError requires a failed lastAttempt",
                    ));
                }
            }
            Some(attempt)
                if matches!(
                    attempt.outcome,
                    AttemptOutcome::Success | AttemptOutcome::Degraded
                ) =>
            {
                if self.last_error.as_ref().is_some() {
                    return Err(ValidationError::new(
                        "successful/degraded lastAttempt must clear lastError",
                    ));
                }
                let success = self.last_success.as_ref().ok_or_else(|| {
                    ValidationError::new("successful/degraded lastAttempt requires lastSuccess")
                })?;
                if attempt.revision.as_ref() != Some(&success.revision)
                    || attempt.generation_id.as_ref() != Some(&success.generation_id)
                    || attempt.offline != success.offline
                    || attempt.promoted != success.promoted
                    || (attempt.outcome == AttemptOutcome::Success
                        && success.compliance != SuccessfulCompliance::Compliant)
                    || (attempt.outcome == AttemptOutcome::Degraded
                        && success.compliance != SuccessfulCompliance::Degraded)
                {
                    return Err(ValidationError::new(
                        "lastSuccess does not describe the successful/degraded lastAttempt",
                    ));
                }
            }
            Some(attempt) => {
                let attempt_error = attempt
                    .error
                    .as_ref()
                    .ok_or_else(|| ValidationError::new("failed lastAttempt requires an error"))?;
                if self.last_error.as_ref() != Some(attempt_error) {
                    return Err(ValidationError::new(
                        "lastError must equal the error from the failed lastAttempt",
                    ));
                }
            }
        }

        match self.selection.current.as_ref() {
            None => {
                if self.last_success.as_ref().is_some()
                    || self.recorded_compliance.as_ref().is_some()
                {
                    return Err(ValidationError::new(
                        "state without current cannot contain lastSuccess or recordedCompliance",
                    ));
                }
            }
            Some(current) => {
                let success = self.last_success.as_ref().ok_or_else(|| {
                    ValidationError::new("current selection requires lastSuccess")
                })?;
                let compliance = self.recorded_compliance.as_ref().ok_or_else(|| {
                    ValidationError::new("current selection requires recordedCompliance")
                })?;
                if success.revision != current.revision
                    || success.generation_id != current.generation_id
                    || compliance.revision != current.revision
                    || compliance.generation_id != current.generation_id
                {
                    return Err(ValidationError::new(
                        "current, lastSuccess, and recordedCompliance must identify the same generation",
                    ));
                }
                match self.source.kind {
                    SourceKind::Local => {
                        if let Freshness::Local { snapshot_sha256 } = &self.freshness {
                            if snapshot_sha256.as_ref() != Some(&current.bundle_sha256) {
                                return Err(ValidationError::new(
                                    "local freshness snapshotSha256 must match current bundle",
                                ));
                            }
                        }
                    }
                    SourceKind::Https => {
                        if let Freshness::Https {
                            high_water_generation,
                            manifest_sha256,
                            revision,
                            artifact_sha256,
                            ..
                        } = &self.freshness
                        {
                            let high_water = high_water_generation.as_ref().ok_or_else(|| {
                                ValidationError::new(
                                    "HTTPS current selection requires a high-water generation",
                                )
                            })?;
                            let selected =
                                current.provider_generation.as_ref().ok_or_else(|| {
                                    ValidationError::new(
                                        "HTTPS current selection requires providerGeneration",
                                    )
                                })?;
                            if selected.get() > high_water.get() {
                                return Err(ValidationError::new(
                                    "HTTPS current generation exceeds its high-water generation",
                                ));
                            }
                            if selected == high_water
                                && (manifest_sha256.as_ref() != current.manifest_sha256.as_ref()
                                    || revision.as_ref() != Some(&current.revision)
                                    || artifact_sha256.as_ref() != current.artifact_sha256.as_ref())
                            {
                                return Err(ValidationError::new(
                                    "HTTPS equal-generation high-water identity does not match current",
                                ));
                            }
                        }
                    }
                    SourceKind::Git => {
                        if let Freshness::Git {
                            accepted_commit,
                            accepted_at,
                            ..
                        } = &self.freshness
                        {
                            if accepted_commit.as_ref().is_none() || accepted_at.as_ref().is_none()
                            {
                                return Err(ValidationError::new(
                                    "Git current selection requires an accepted commit and timestamp",
                                ));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_successor(&self, previous: &Self) -> Result<(), ValidationError> {
        self.validate()?;
        previous.validate()?;
        if self.source != previous.source {
            return Err(ValidationError::new(
                "state successor cannot change canonical source identity",
            ));
        }
        if self.updated_at < previous.updated_at {
            return Err(ValidationError::new("state updatedAt cannot move backward"));
        }
        let expected = previous
            .snapshot_sequence
            .get()
            .checked_add(1)
            .filter(|value| *value <= MAX_SAFE_INTEGER)
            .ok_or_else(|| ValidationError::new("snapshotSequence overflow"))?;
        if self.snapshot_sequence.get() != expected {
            return Err(ValidationError::new(format!(
                "snapshotSequence must advance from {} to {expected}",
                previous.snapshot_sequence.get()
            )));
        }

        let explicit_successful_rollback = self.last_attempt.as_ref().is_some_and(|attempt| {
            attempt.rollback
                && !attempt.offline
                && matches!(
                    attempt.outcome,
                    AttemptOutcome::Success | AttemptOutcome::Degraded
                )
        });

        match (&previous.freshness, &self.freshness) {
            (
                Freshness::Https {
                    high_water_generation: previous_generation,
                    manifest_sha256: previous_manifest,
                    revision: previous_revision,
                    artifact_sha256: previous_artifact,
                    last_online_contact: previous_contact,
                    ..
                },
                Freshness::Https {
                    high_water_generation: next_generation,
                    manifest_sha256: next_manifest,
                    revision: next_revision,
                    artifact_sha256: next_artifact,
                    last_online_contact: next_contact,
                    ..
                },
            ) => {
                match (previous_generation.as_ref(), next_generation.as_ref()) {
                    (Some(_), None) => {
                        return Err(ValidationError::new(
                            "HTTPS high-water state cannot be cleared",
                        ))
                    }
                    (Some(previous_value), Some(next_value))
                        if next_value.get() < previous_value.get() =>
                    {
                        return Err(ValidationError::new(
                            "HTTPS high-water generation cannot decrease",
                        ));
                    }
                    (Some(previous_value), Some(next_value))
                        if next_value == previous_value
                            && (next_manifest != previous_manifest
                                || next_revision != previous_revision
                                || next_artifact != previous_artifact) =>
                    {
                        return Err(ValidationError::new(
                            "equal HTTPS generations must retain identical authenticated content",
                        ));
                    }
                    _ => {}
                }
                if previous_contact.as_ref().is_some()
                    && (next_contact.as_ref().is_none()
                        || next_contact.as_ref() < previous_contact.as_ref())
                {
                    return Err(ValidationError::new(
                        "HTTPS lastOnlineContact cannot be cleared or move backward",
                    ));
                }
            }
            (
                Freshness::Git {
                    accepted_commit: previous_commit,
                    accepted_tag_object: previous_tag,
                    accepted_at: previous_at,
                    ..
                },
                Freshness::Git {
                    accepted_commit: next_commit,
                    accepted_tag_object: next_tag,
                    accepted_at: next_at,
                    ..
                },
            ) => {
                if previous_commit.as_ref().is_some() && next_commit.as_ref().is_none() {
                    return Err(ValidationError::new(
                        "Git accepted high-water commit cannot be cleared",
                    ));
                }
                let tag_selector = matches!(
                    &previous.source.canonical,
                    CanonicalSource::Git { selector, .. }
                        if selector.as_str().starts_with("refs/tags/")
                );
                if tag_selector
                    && previous_commit.as_ref().is_some()
                    && (previous_commit != next_commit || previous_tag != next_tag)
                    && !explicit_successful_rollback
                {
                    return Err(ValidationError::new(
                        "an observed Git tag binding is immutable",
                    ));
                }
                if previous_commit == next_commit && previous_tag != next_tag {
                    return Err(ValidationError::new(
                        "an accepted Git commit cannot change its tag-object binding",
                    ));
                }
                if previous_at.as_ref().is_some()
                    && (next_at.as_ref().is_none() || next_at.as_ref() < previous_at.as_ref())
                {
                    return Err(ValidationError::new(
                        "Git acceptedAt cannot be cleared or move backward",
                    ));
                }
            }
            (Freshness::Local { .. }, Freshness::Local { .. }) => {}
            _ => {
                return Err(ValidationError::new(
                    "state successor cannot change freshness provider kind",
                ))
            }
        }

        if self.source.kind == SourceKind::Git {
            let current_changed = self
                .selection
                .current
                .as_ref()
                .map(|generation| (generation.generation_id, generation.revision.as_str()))
                != previous
                    .selection
                    .current
                    .as_ref()
                    .map(|generation| (generation.generation_id, generation.revision.as_str()));
            if current_changed && !explicit_successful_rollback {
                if let (
                    Some(current),
                    Freshness::Git {
                        accepted_commit, ..
                    },
                ) = (self.selection.current.as_ref(), &self.freshness)
                {
                    if accepted_commit
                        .as_ref()
                        .is_none_or(|commit| commit.as_str() != current.revision.as_str())
                    {
                        return Err(ValidationError::new(
                            "a non-rollback Git selection must promote the accepted commit",
                        ));
                    }
                }
            }
        }

        if let Some(attempt) = self.last_attempt.as_ref() {
            let attempted_current = attempt.generation_id.as_ref().is_some_and(|id| {
                previous
                    .selection
                    .current
                    .as_ref()
                    .is_some_and(|current| &current.generation_id == id)
            });
            let failed_candidate = matches!(
                attempt.outcome,
                AttemptOutcome::Noncompliant | AttemptOutcome::Error
            ) && !attempted_current;
            if failed_candidate
                && (self.selection != previous.selection
                    || self.recorded_compliance != previous.recorded_compliance)
            {
                return Err(ValidationError::new(
                    "failed candidate must preserve selection and recorded compliance",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AuditAction {
    Apply,
    Rollback,
    Enroll,
    Unenroll,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AuditOutcome {
    Success,
    Rejected,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AuditRecord {
    pub(crate) audit_id: Hash256,
    pub(crate) at: Timestamp,
    pub(crate) actor: String,
    pub(crate) action: AuditAction,
    pub(crate) source_id: SourceId,
    pub(crate) from_generation_id: RequiredNullable<GenerationId>,
    pub(crate) to_generation_id: RequiredNullable<GenerationId>,
    pub(crate) from_revision: RequiredNullable<Revision>,
    pub(crate) to_revision: RequiredNullable<Revision>,
    pub(crate) reason: RequiredNullable<String>,
    pub(crate) outcome: AuditOutcome,
}

impl AuditRecord {
    pub(crate) fn decode_strict(bytes: &[u8]) -> Result<Self, DecodeError> {
        let record: Self = from_strict_value(strict_json_value(bytes)?)?;
        record.validate().map_err(validation_decode)?;
        Ok(record)
    }

    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        validate_bounded_text(&self.actor, "audit actor", 1, 256, false)?;
        if self.from_generation_id.is_null() != self.from_revision.is_null()
            || self.to_generation_id.is_null() != self.to_revision.is_null()
        {
            return Err(ValidationError::new(
                "audit generation IDs and diagnostic revisions must be null or present in pairs",
            ));
        }
        if let Some(reason) = self.reason.as_ref() {
            if reason.len() > 512 {
                return Err(ValidationError::new("audit reason exceeds 512 UTF-8 bytes"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FailureRecord {
    pub(crate) failure_id: Hash256,
    pub(crate) attempt_id: Hash256,
    pub(crate) at: Timestamp,
    pub(crate) error: StateErrorRecord,
    pub(crate) captured_output: RequiredNullable<PersistedCapturedOutput>,
}

impl FailureRecord {
    pub(crate) fn decode_strict(bytes: &[u8]) -> Result<Self, DecodeError> {
        let record: Self = from_strict_value(strict_json_value(bytes)?)?;
        record.validate().map_err(validation_decode)?;
        Ok(record)
    }

    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        self.error.validate()?;
        if let Some(output) = self.captured_output.as_ref() {
            output.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: u8) -> Hash256 {
        Hash256::from_bytes([byte; 32])
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

    fn timestamp() -> Timestamp {
        Timestamp::parse("2026-07-21T00:00:00.000Z").unwrap()
    }

    #[test]
    fn primitive_types_enforce_contract_boundaries() {
        assert!(SafeInteger::new(MAX_SAFE_INTEGER).is_ok());
        assert!(SafeInteger::new(MAX_SAFE_INTEGER + 1).is_err());
        assert!(SafeInteger::positive(0).is_err());
        assert!(Timestamp::parse("2026-07-21T00:00:00.000Z").is_ok());
        assert!(Timestamp::parse("2026-07-21T00:00:00Z").is_err());
        assert!(Timestamp::parse("2026-02-30T00:00:00.000Z").is_err());
        assert!(NormalizedRelativePath::parse("profiles/laptop.yaml").is_ok());
        for rejected in ["", "/absolute", "a//b", "a/../b", "a\\b", "a/"] {
            assert!(
                NormalizedRelativePath::parse(rejected).is_err(),
                "{rejected:?}"
            );
        }
    }

    #[test]
    fn native_paths_use_lossless_unpadded_base64url() {
        let path = NativePath::from_bytes(b"/tmp/checksy".to_vec()).unwrap();
        let json = serde_json::to_string(&path).unwrap();
        assert_eq!(
            json,
            r#"{"bytesBase64Url":"L3RtcC9jaGVja3N5","display":"/tmp/checksy"}"#
        );
        assert_eq!(serde_json::from_str::<NativePath>(&json).unwrap(), path);
        assert!(serde_json::from_str::<NativePath>(
            r#"{"bytesBase64Url":"L3RtcA==","display":"/tmp"}"#
        )
        .is_err());
    }

    #[test]
    fn marker_is_content_only_and_validates_derived_identity() {
        let source = local_source();
        let bundle = hash(7);
        let config_path = NormalizedRelativePath::parse(".checksy.yaml").unwrap();
        let generation_id =
            GenerationIdentity::local(source.id, bundle, config_path.as_str().to_string())
                .generation_id();
        let marker = GenerationMarker {
            schema_version: SafeInteger::positive(1).unwrap(),
            completed: true,
            source_id: source.id,
            generation_id,
            config_path,
            bundle_sha256: bundle,
            provider: MarkerProvider::Local,
        };
        marker.validate_for_source(&source).unwrap();
        let encoded = serde_json::to_vec(&marker).unwrap();
        assert_eq!(GenerationMarker::decode_strict(&encoded).unwrap(), marker);

        let mut value = serde_json::to_value(&marker).unwrap();
        value["completed"] = Value::Bool(false);
        assert!(GenerationMarker::decode_strict(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn marker_contract_fixtures_match_runtime_identity_validation() {
        for fixture in [
            include_bytes!(
                "../../fixtures/pull-agent-contract/state-store/markers/valid/local.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/state-store/markers/valid/git-sha1.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/state-store/markers/valid/git-sha256.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/state-store/markers/valid/https.json"
            )
            .as_slice(),
        ] {
            GenerationMarker::decode_strict(fixture).unwrap();
        }

        for fixture in [
            include_bytes!(
                "../../fixtures/pull-agent-contract/state-store/markers/invalid/completed-false.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/state-store/markers/invalid/git-format-length.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/state-store/markers/invalid/signer-in-marker.json"
            )
            .as_slice(),
        ] {
            assert!(GenerationMarker::decode_strict(fixture).is_err());
        }
    }

    #[test]
    fn strict_json_rejects_duplicate_bom_trailing_and_unknown_fields() {
        let source = local_source();
        let source_json = serde_json::to_string(&source).unwrap();
        let valid = format!(
            r#"{{"schemaVersion":1,"snapshotSequence":1,"source":{source_json},"selection":{{"current":null,"previous":null,"additional":[]}},"freshness":{{"kind":"local","snapshotSha256":null}},"lastAttempt":null,"lastSuccess":null,"lastError":null,"recordedCompliance":null,"updatedAt":"2026-07-21T00:00:00.000Z"}}"#
        );
        StateSnapshot::decode_strict(valid.as_bytes()).unwrap();

        let duplicate = valid.replacen(
            r#""snapshotSequence":1"#,
            r#""snapshotSequence":1,"snapshotSequence":2"#,
            1,
        );
        assert!(StateSnapshot::decode_strict(duplicate.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("duplicate"));
        assert!(StateSnapshot::decode_strict(
            &[b"\xef\xbb\xbf".as_slice(), valid.as_bytes()].concat()
        )
        .is_err());
        assert!(StateSnapshot::decode_strict(format!("{valid} {{}}").as_bytes()).is_err());
        let unknown = valid.replacen(
            r#""schemaVersion":1"#,
            r#""schemaVersion":1,"unknown":true"#,
            1,
        );
        assert!(StateSnapshot::decode_strict(unknown.as_bytes()).is_err());
    }

    #[test]
    fn required_nullable_rejects_an_omitted_property() {
        let source = local_source();
        let source_json = serde_json::to_string(&source).unwrap();
        let missing = format!(
            r#"{{"schemaVersion":1,"snapshotSequence":1,"source":{source_json},"selection":{{"current":null,"previous":null,"additional":[]}},"freshness":{{"kind":"local","snapshotSha256":null}},"lastSuccess":null,"lastError":null,"recordedCompliance":null,"updatedAt":"2026-07-21T00:00:00.000Z"}}"#
        );
        assert!(StateSnapshot::decode_strict(missing.as_bytes()).is_err());
    }

    #[test]
    fn compliance_aggregates_are_checked() {
        let compliance = Compliance {
            state: ComplianceState::Degraded,
            revision: Revision::parse("local-1").unwrap(),
            generation_id: GenerationId::from_hash(hash(1)),
            checked_at: timestamp(),
            offline: false,
            fail_severity: StateSeverity::Error,
            failing_rules: SafeInteger::new(0).unwrap(),
            degraded_rules: SafeInteger::new(1).unwrap(),
            counts: SeverityCounts {
                debug: SafeInteger::new(0).unwrap(),
                info: SafeInteger::new(0).unwrap(),
                warn: SafeInteger::new(1).unwrap(),
                error: SafeInteger::new(0).unwrap(),
            },
        };
        compliance.validate().unwrap();
        let mut invalid = compliance;
        invalid.failing_rules = SafeInteger::new(1).unwrap();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn successor_preserves_https_high_water_and_rejects_equivocation() {
        let canonical = CanonicalSource::Https {
            manifest_url: "https://example.test/checksy/manifest.json".to_string(),
        };
        let source = StateSource {
            id: canonical.identity().unwrap().source_id(),
            kind: SourceKind::Https,
            display: "https://example.test/checksy/manifest.json".to_string(),
            canonical,
        };
        let freshness = Freshness::Https {
            high_water_generation: RequiredNullable::some(SafeInteger::positive(7).unwrap()),
            manifest_sha256: RequiredNullable::some(hash(1)),
            revision: RequiredNullable::some(Revision::parse("release-7").unwrap()),
            artifact_sha256: RequiredNullable::some(hash(2)),
            etag: RequiredNullable::null(),
            last_modified: RequiredNullable::null(),
            last_online_contact: RequiredNullable::some(
                Timestamp::parse("2026-07-21T00:00:00.000Z").unwrap(),
            ),
        };
        let previous = StateSnapshot {
            schema_version: SafeInteger::positive(1).unwrap(),
            snapshot_sequence: SafeInteger::positive(1).unwrap(),
            source,
            selection: Selection::empty(),
            freshness,
            last_attempt: RequiredNullable::null(),
            last_success: RequiredNullable::null(),
            last_error: RequiredNullable::null(),
            recorded_compliance: RequiredNullable::null(),
            updated_at: Timestamp::parse("2026-07-21T00:00:00.000Z").unwrap(),
        };
        previous.validate().unwrap();

        let mut next = previous.clone();
        next.snapshot_sequence = SafeInteger::positive(2).unwrap();
        next.updated_at = Timestamp::parse("2026-07-21T00:00:01.000Z").unwrap();
        next.validate_successor(&previous).unwrap();

        if let Freshness::Https {
            high_water_generation,
            ..
        } = &mut next.freshness
        {
            *high_water_generation = RequiredNullable::some(SafeInteger::positive(6).unwrap());
        }
        assert!(next
            .validate_successor(&previous)
            .unwrap_err()
            .to_string()
            .contains("cannot decrease"));

        next.freshness = previous.freshness.clone();
        if let Freshness::Https {
            manifest_sha256, ..
        } = &mut next.freshness
        {
            *manifest_sha256 = RequiredNullable::some(hash(3));
        }
        assert!(next
            .validate_successor(&previous)
            .unwrap_err()
            .to_string()
            .contains("identical authenticated content"));
    }

    #[test]
    fn git_pins_current_high_water_and_observed_tags_are_bound() {
        let pinned_oid = "1111111111111111111111111111111111111111";
        let config_path = NormalizedRelativePath::parse("checks/checksy.yaml").unwrap();
        let pinned_selector = GitSelector::parse(&format!("oid:{pinned_oid}")).unwrap();
        let pinned_canonical = CanonicalSource::Git {
            repository: "https://example.test/checksy.git".to_string(),
            selector: pinned_selector.clone(),
            config_path: config_path.clone(),
        };
        let pinned_source = StateSource {
            id: pinned_canonical.identity().unwrap().source_id(),
            kind: SourceKind::Git,
            display: "pinned Git source".to_string(),
            canonical: pinned_canonical,
        };
        let mismatched = Freshness::Git {
            selector: pinned_selector,
            accepted_commit: RequiredNullable::some(
                GitObjectId::parse("2222222222222222222222222222222222222222").unwrap(),
            ),
            accepted_tag_object: RequiredNullable::null(),
            accepted_at: RequiredNullable::some(timestamp()),
        };
        assert!(mismatched
            .validate_for_source(&pinned_source)
            .unwrap_err()
            .to_string()
            .contains("full-OID"));

        let revision = Revision::parse(pinned_oid).unwrap();
        let generation_id = GenerationIdentity::git(
            pinned_source.id,
            ObjectFormat::Sha1,
            pinned_oid.to_string(),
            config_path.as_str().to_string(),
        )
        .generation_id();
        let generation = Generation {
            generation_id,
            revision: revision.clone(),
            config_path,
            provider_generation: RequiredNullable::null(),
            manifest_sha256: RequiredNullable::null(),
            artifact_sha256: RequiredNullable::null(),
            bundle_sha256: hash(9),
            signer: Signer::GitContentPin {
                object_id: GitObjectId::parse(pinned_oid).unwrap(),
            },
            verified_at: timestamp(),
            promoted_at: timestamp(),
        };
        let success = Success {
            at: timestamp(),
            revision: revision.clone(),
            generation_id,
            compliance: SuccessfulCompliance::Compliant,
            offline: false,
            promoted: true,
        };
        let compliance = Compliance {
            state: ComplianceState::Compliant,
            revision,
            generation_id,
            checked_at: timestamp(),
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
        let current_without_high_water = StateSnapshot {
            schema_version: SafeInteger::positive(1).unwrap(),
            snapshot_sequence: SafeInteger::positive(1).unwrap(),
            source: pinned_source,
            selection: Selection {
                current: RequiredNullable::some(generation),
                previous: RequiredNullable::null(),
                additional: Vec::new(),
            },
            freshness: Freshness::Git {
                selector: GitSelector::parse(&format!("oid:{pinned_oid}")).unwrap(),
                accepted_commit: RequiredNullable::null(),
                accepted_tag_object: RequiredNullable::null(),
                accepted_at: RequiredNullable::null(),
            },
            last_attempt: RequiredNullable::null(),
            last_success: RequiredNullable::some(success),
            last_error: RequiredNullable::null(),
            recorded_compliance: RequiredNullable::some(compliance),
            updated_at: timestamp(),
        };
        assert!(current_without_high_water
            .validate()
            .unwrap_err()
            .to_string()
            .contains("accepted commit"));

        let tag_selector = GitSelector::parse("refs/tags/release").unwrap();
        let tag_canonical = CanonicalSource::Git {
            repository: "https://example.test/checksy.git".to_string(),
            selector: tag_selector.clone(),
            config_path: NormalizedRelativePath::parse("checks/checksy.yaml").unwrap(),
        };
        let tag_source = StateSource {
            id: tag_canonical.identity().unwrap().source_id(),
            kind: SourceKind::Git,
            display: "tagged Git source".to_string(),
            canonical: tag_canonical,
        };
        let tag_freshness = Freshness::Git {
            selector: tag_selector,
            accepted_commit: RequiredNullable::some(
                GitObjectId::parse("3333333333333333333333333333333333333333").unwrap(),
            ),
            accepted_tag_object: RequiredNullable::some(
                GitObjectId::parse("4444444444444444444444444444444444444444").unwrap(),
            ),
            accepted_at: RequiredNullable::some(timestamp()),
        };
        let previous = StateSnapshot {
            schema_version: SafeInteger::positive(1).unwrap(),
            snapshot_sequence: SafeInteger::positive(1).unwrap(),
            source: tag_source,
            selection: Selection::empty(),
            freshness: tag_freshness,
            last_attempt: RequiredNullable::null(),
            last_success: RequiredNullable::null(),
            last_error: RequiredNullable::null(),
            recorded_compliance: RequiredNullable::null(),
            updated_at: timestamp(),
        };
        let mut retargeted = previous.clone();
        retargeted.snapshot_sequence = SafeInteger::positive(2).unwrap();
        if let Freshness::Git {
            accepted_commit,
            accepted_tag_object,
            ..
        } = &mut retargeted.freshness
        {
            *accepted_commit = RequiredNullable::some(
                GitObjectId::parse("5555555555555555555555555555555555555555").unwrap(),
            );
            *accepted_tag_object = RequiredNullable::some(
                GitObjectId::parse("6666666666666666666666666666666666666666").unwrap(),
            );
        }
        assert!(retargeted
            .validate_successor(&previous)
            .unwrap_err()
            .to_string()
            .contains("immutable"));
    }

    #[test]
    fn unsupported_schema_version_is_distinct() {
        let source = local_source();
        let source_json = serde_json::to_string(&source).unwrap();
        let value = format!(
            r#"{{"schemaVersion":2,"snapshotSequence":1,"source":{source_json},"selection":{{"current":null,"previous":null,"additional":[]}},"freshness":{{"kind":"local","snapshotSha256":null}},"lastAttempt":null,"lastSuccess":null,"lastError":null,"recordedCompliance":null,"updatedAt":"2026-07-21T00:00:00.000Z"}}"#
        );
        assert_eq!(
            StateSnapshot::decode_strict(value.as_bytes()).unwrap_err(),
            DecodeError::UnsupportedSchemaVersion(2)
        );
    }

    #[test]
    fn audit_generation_ids_are_required_explicit_nullable_fields() {
        let fixture = include_bytes!(
            "../../fixtures/pull-agent-contract/formats/state/records/valid/audit-rollback.json"
        );
        AuditRecord::decode_strict(fixture).unwrap();

        let mut value: Value = serde_json::from_slice(fixture).unwrap();
        value.as_object_mut().unwrap().remove("fromGenerationId");
        assert!(AuditRecord::decode_strict(&serde_json::to_vec(&value).unwrap()).is_err());

        value["fromGenerationId"] = Value::Null;
        assert!(AuditRecord::decode_strict(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn state_and_record_contract_fixtures_match_runtime_validation() {
        for fixture in [
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/valid/degraded-local.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/valid/git-content-pin.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/valid/git-ssh.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/valid/https-failed-candidate.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/valid/https-offline-success.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/valid/https-rollback.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/valid/no-current-local.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/valid/no-current-signed-http-direct.json"
            )
            .as_slice(),
        ] {
            // The long-standing public-format fixtures intentionally use
            // illustrative digest placeholders. They remain authoritative for
            // the closed wire shape, while the state-store marker fixtures and
            // constructed snapshots exercise derived-ID semantics.
            let value = strict_json_value(fixture).unwrap();
            let _: StateSnapshot = from_strict_value(value).unwrap();
        }

        for fixture in [
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/invalid/additional-over-limit.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/invalid/corrupt.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/invalid/current-without-convergence.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/invalid/timestamp-no-milliseconds.json"
            )
            .as_slice(),
        ] {
            assert!(StateSnapshot::decode_strict(fixture).is_err());
        }

        for fixture in [
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/records/valid/audit-initial-apply.json"
            )
            .as_slice(),
            include_bytes!(
                "../../fixtures/pull-agent-contract/formats/state/records/valid/audit-rollback.json"
            )
            .as_slice(),
        ] {
            AuditRecord::decode_strict(fixture).unwrap();
        }
        FailureRecord::decode_strict(include_bytes!(
            "../../fixtures/pull-agent-contract/formats/state/records/valid/failure-output.json"
        ))
        .unwrap();

        assert!(AuditRecord::decode_strict(include_bytes!(
            "../../fixtures/pull-agent-contract/formats/state/records/invalid/audit-missing-generation-id.json"
        ))
        .is_err());
        assert!(FailureRecord::decode_strict(include_bytes!(
            "../../fixtures/pull-agent-contract/formats/state/records/invalid/failure-output-padding.json"
        ))
        .is_err());
    }
}
