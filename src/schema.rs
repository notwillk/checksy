pub mod schema {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Optional fields in configuration remain optional, but an explicitly
    /// present YAML value must have the declared type. In particular, `null`
    /// is not treated as if the field had been omitted.
    struct StrictOptional<T>(Option<T>);

    impl<T> Default for StrictOptional<T> {
        fn default() -> Self {
            Self(None)
        }
    }

    impl<'de, T> Deserialize<'de> for StrictOptional<T>
    where
        T: Deserialize<'de>,
    {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            T::deserialize(deserializer).map(|value| Self(Some(value)))
        }
    }

    impl<T> StrictOptional<T> {
        fn into_option(self) -> Option<T> {
            self.0
        }
    }

    struct StrictString(String);

    impl StrictString {
        fn into_string(self) -> String {
            self.0
        }
    }

    impl<'de> Deserialize<'de> for StrictString {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            match serde_yaml::Value::deserialize(deserializer)? {
                serde_yaml::Value::String(value) => Ok(Self(value)),
                _ => Err(serde::de::Error::custom(
                    "invalid type: expected a YAML string",
                )),
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum Severity {
        #[default]
        Error,
        Warning,
        Info,
        Debug,
    }

    impl Serialize for Severity {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let s = match self {
                Severity::Error => "error",
                Severity::Warning => "warn",
                Severity::Info => "info",
                Severity::Debug => "debug",
            };
            serializer.serialize_str(s)
        }
    }

    impl<'de> Deserialize<'de> for Severity {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s = StrictString::deserialize(deserializer)?.into_string();
            match s.to_lowercase().as_str() {
                "error" => Ok(Severity::Error),
                "warn" | "warning" => Ok(Severity::Warning),
                "info" => Ok(Severity::Info),
                "debug" => Ok(Severity::Debug),
                _ => Err(serde::de::Error::custom(format!(
                    "unknown variant `{}`, expected one of `error`, `warning`, `info`, `debug`",
                    s
                ))),
            }
        }
    }

    impl Severity {
        pub fn normalize(&self) -> Self {
            *self
        }

        pub fn parse(value: &str) -> Option<Severity> {
            match value.to_lowercase().trim() {
                "error" => Some(Severity::Error),
                "warn" | "warning" => Some(Severity::Warning),
                "info" => Some(Severity::Info),
                "debug" => Some(Severity::Debug),
                _ => None,
            }
        }
    }

    impl std::fmt::Display for Severity {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Severity::Error => write!(f, "error"),
                Severity::Warning => write!(f, "warn"),
                Severity::Info => write!(f, "info"),
                Severity::Debug => write!(f, "debug"),
            }
        }
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct Rule {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub check: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub severity: Option<Severity>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub fix: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub hint: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub remote: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawRule {
        #[serde(default)]
        name: StrictOptional<StrictString>,
        #[serde(default)]
        check: StrictOptional<StrictString>,
        #[serde(default)]
        severity: StrictOptional<Severity>,
        #[serde(default)]
        fix: StrictOptional<StrictString>,
        #[serde(default)]
        hint: StrictOptional<StrictString>,
        #[serde(default)]
        remote: StrictOptional<StrictString>,
    }

    impl TryFrom<RawRule> for Rule {
        type Error = String;

        fn try_from(raw: RawRule) -> Result<Self, Self::Error> {
            let rule = Self {
                name: raw.name.into_option().map(StrictString::into_string),
                check: raw.check.into_option().map(StrictString::into_string),
                severity: raw.severity.into_option(),
                fix: raw.fix.into_option().map(StrictString::into_string),
                hint: raw.hint.into_option().map(StrictString::into_string),
                remote: raw.remote.into_option().map(StrictString::into_string),
            };
            rule.validate()?;
            Ok(rule)
        }
    }

    impl<'de> Deserialize<'de> for Rule {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let raw = RawRule::deserialize(deserializer)?;
            Rule::try_from(raw).map_err(serde::de::Error::custom)
        }
    }

    impl Rule {
        /// Validate the two supported rule forms. A rule is either a remote
        /// reference with no command metadata, or an executable rule with a
        /// non-empty check command.
        pub fn validate(&self) -> Result<(), String> {
            if self.is_remote() {
                if let Some(error) = self.validate_remote_only() {
                    return Err(error);
                }

                let remote = self.remote.as_deref().unwrap_or_default();
                if remote.trim().is_empty() {
                    return Err("remote rule requires a non-empty `remote` value".to_string());
                }
                if remote.contains('\0') {
                    return Err("remote rule contains a NUL byte".to_string());
                }
                return Ok(());
            }

            let Some(check) = self.check.as_deref() else {
                if self.fix.is_some() {
                    return Err("inline rule cannot define `fix` without `check`".to_string());
                }
                return Err(
                    "rule must contain exactly one of `remote` or a non-empty `check`".to_string(),
                );
            };

            if check.trim().is_empty() {
                return Err("inline rule requires a non-empty `check` value".to_string());
            }
            if check.contains('\0') || self.fix.as_deref().is_some_and(|fix| fix.contains('\0')) {
                return Err("inline rule commands cannot contain NUL bytes".to_string());
            }

            Ok(())
        }

        /// Returns true if this is a remote rule (has remote property set)
        pub fn is_remote(&self) -> bool {
            self.remote.is_some()
        }

        /// Validates that a remote rule only has the remote property set
        /// Returns None if valid, Some(error_message) if invalid
        pub fn validate_remote_only(&self) -> Option<String> {
            if !self.is_remote() {
                return None;
            }

            let mut invalid_props = Vec::new();

            if self.name.is_some() {
                invalid_props.push("name");
            }
            if self.check.is_some() {
                invalid_props.push("check");
            }
            if self.severity.is_some() {
                invalid_props.push("severity");
            }
            if self.fix.is_some() {
                invalid_props.push("fix");
            }
            if self.hint.is_some() {
                invalid_props.push("hint");
            }

            if !invalid_props.is_empty() {
                Some(format!(
                    "remote rule cannot have properties: {}",
                    invalid_props.join(", ")
                ))
            } else {
                None
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Default)]
    #[serde(rename_all = "camelCase")]
    pub struct Config {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub cache_path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub check_severity: Option<Severity>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub fail_severity: Option<Severity>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub preconditions: Vec<Rule>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub rules: Vec<Rule>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub patterns: Vec<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct RawConfig {
        #[serde(default)]
        cache_path: StrictOptional<StrictString>,
        #[serde(default)]
        check_severity: StrictOptional<Severity>,
        #[serde(default)]
        fail_severity: StrictOptional<Severity>,
        #[serde(default)]
        preconditions: Vec<Rule>,
        #[serde(default)]
        rules: Vec<Rule>,
        #[serde(default)]
        patterns: Vec<StrictString>,
    }

    impl<'de> Deserialize<'de> for Config {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let raw = RawConfig::deserialize(deserializer)?;
            let config = Self {
                cache_path: raw.cache_path.into_option().map(StrictString::into_string),
                check_severity: raw.check_severity.into_option(),
                fail_severity: raw.fail_severity.into_option(),
                preconditions: raw.preconditions,
                rules: raw.rules,
                patterns: raw
                    .patterns
                    .into_iter()
                    .map(StrictString::into_string)
                    .collect(),
            };
            config.validate().map_err(serde::de::Error::custom)?;
            Ok(config)
        }
    }

    impl Config {
        pub fn validate(&self) -> Result<(), String> {
            if self
                .cache_path
                .as_deref()
                .is_some_and(|path| path.contains('\0'))
            {
                return Err("`cachePath` cannot contain a NUL byte".to_string());
            }

            for (index, rule) in self.preconditions.iter().enumerate() {
                rule.validate()
                    .map_err(|error| format!("preconditions[{}]: {}", index, error))?;
            }
            for (index, rule) in self.rules.iter().enumerate() {
                rule.validate()
                    .map_err(|error| format!("rules[{}]: {}", index, error))?;
            }
            for (index, pattern) in self.patterns.iter().enumerate() {
                let pattern = pattern.trim();
                let pattern = pattern.strip_prefix('!').unwrap_or(pattern).trim();
                if pattern.is_empty() {
                    return Err(format!("patterns[{}] must not be empty", index));
                }
                glob::Pattern::new(pattern)
                    .map_err(|error| format!("patterns[{}] is invalid: {}", index, error))?;
            }

            Ok(())
        }
    }

    pub fn severity_order(sev: Severity) -> u8 {
        match sev {
            Severity::Debug => 0,
            Severity::Info => 1,
            Severity::Warning => 2,
            Severity::Error => 3,
        }
    }
}

pub use schema::*;
