use crate::cache::GitRemote;
use crate::config::ConfigDiagnostic;
use crate::schema::{Config, Rule, Severity};
use std::path::PathBuf;

/// Structured identity of the selected definition source for one resolution run.
///
/// Filesystem members are canonicalized, while complete Git endpoint/selector
/// normalization, persistent source IDs, and their versioned hash encoding
/// belong to the future source-provider and state layers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum SourceIdentity {
    Local {
        root: PathBuf,
    },
    Git {
        repository: String,
        selector: String,
        checkout: PathBuf,
    },
    Stdin {
        working_directory: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefinitionOrigin {
    pub(crate) source_identity: SourceIdentity,
    pub(crate) defining_config_path: PathBuf,
    pub(crate) source_relative_config: Option<PathBuf>,
    pub(crate) base_dir: PathBuf,
    pub(crate) bundle_root: Option<PathBuf>,
    pub(crate) revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct DefinitionKey {
    pub(crate) source_identity: SourceIdentity,
    pub(crate) revision: Option<String>,
    pub(crate) defining_config_path: PathBuf,
}

impl From<&DefinitionOrigin> for DefinitionKey {
    fn from(origin: &DefinitionOrigin) -> Self {
        Self {
            source_identity: origin.source_identity.clone(),
            revision: origin.revision.clone(),
            defining_config_path: origin.defining_config_path.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedRule {
    pub(crate) rule: Rule,
    pub(crate) origin: DefinitionOrigin,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedPatternGroup {
    pub(crate) patterns: Vec<String>,
    pub(crate) origin: DefinitionOrigin,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ResolvedDefinition {
    pub(crate) cache_path: Option<String>,
    pub(crate) check_severity: Option<Severity>,
    pub(crate) fail_severity: Option<Severity>,
    pub(crate) preconditions: Vec<ResolvedRule>,
    pub(crate) rules: Vec<ResolvedRule>,
    pub(crate) pattern_groups: Vec<ResolvedPatternGroup>,
}

impl ResolvedDefinition {
    /// Preserve the existing public `load() -> Config` projection. The public
    /// configuration type cannot carry per-definition origins, so only the root
    /// pattern group is projected; origin-aware command paths use this resolved
    /// representation directly.
    pub(crate) fn legacy_config(&self) -> Config {
        Config {
            cache_path: self.cache_path.clone(),
            check_severity: self.check_severity,
            fail_severity: self.fail_severity,
            preconditions: self
                .preconditions
                .iter()
                .map(|resolved| resolved.rule.clone())
                .collect(),
            rules: self
                .rules
                .iter()
                .map(|resolved| resolved.rule.clone())
                .collect(),
            patterns: self
                .pattern_groups
                .first()
                .map(|group| group.patterns.clone())
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolverMode {
    CachedOnly,
    CacheMissing,
    RefreshOrClone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitDependency {
    pub(crate) remote: GitRemote,
    pub(crate) cache_root: PathBuf,
    pub(crate) cached: bool,
}

#[derive(Debug)]
pub(crate) struct ResolvedLoad {
    pub(crate) definition: ResolvedDefinition,
    pub(crate) diagnostics: Vec<ConfigDiagnostic>,
    /// Every encountered Git dependency in deterministic first-seen order.
    /// Cached dependencies are included so `install` can refresh them; missing
    /// dependencies let callers acquire a parent and repeat discovery.
    pub(crate) git_dependencies: Vec<GitDependency>,
}
