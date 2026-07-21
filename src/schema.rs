pub mod schema {
    use schemars::{generate::SchemaSettings, JsonSchema, Schema, SchemaGenerator};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::borrow::Cow;

    const JSON_SCHEMA_WHITESPACE_CLASS: &str = concat!(
        r"\u0009-\u000D",
        r"\u0020",
        r"\u0085",
        r"\u00A0",
        r"\u1680",
        r"\u2000-\u200A",
        r"\u2028-\u2029",
        r"\u202F",
        r"\u205F",
        r"\u3000"
    );

    const NO_NUL_PATTERN: &str = r"^[^\u0000]*$";

    fn no_nul_string_schema(_generator: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": NO_NUL_PATTERN
        })
    }

    fn non_blank_no_nul_string_schema(_generator: &mut SchemaGenerator) -> Schema {
        let has_non_whitespace = format!("[^{}]", JSON_SCHEMA_WHITESPACE_CLASS);
        schemars::json_schema!({
            "type": "string",
            "allOf": [
                { "pattern": NO_NUL_PATTERN },
                { "pattern": has_non_whitespace }
            ]
        })
    }

    fn pattern_string_schema(_generator: &mut SchemaGenerator) -> Schema {
        let ordinary = format!("^[{0}]*[^!{0}]", JSON_SCHEMA_WHITESPACE_CLASS);
        let negated = format!("^[{0}]*![{0}]*[^{0}]", JSON_SCHEMA_WHITESPACE_CLASS);
        schemars::json_schema!({
            "type": "string",
            "allOf": [{ "pattern": NO_NUL_PATTERN }],
            "anyOf": [
                { "pattern": ordinary },
                { "pattern": negated }
            ]
        })
    }

    /// Optional fields in configuration remain optional, but an explicitly
    /// present YAML value must have the declared type. In particular, `null`
    /// is not treated as if the field had been omitted.
    struct StrictOptional<T>(Option<T>);

    impl<T> JsonSchema for StrictOptional<T>
    where
        T: JsonSchema,
    {
        fn inline_schema() -> bool {
            T::inline_schema()
        }

        fn schema_name() -> Cow<'static, str> {
            T::schema_name()
        }

        fn schema_id() -> Cow<'static, str> {
            T::schema_id()
        }

        fn json_schema(generator: &mut SchemaGenerator) -> Schema {
            T::json_schema(generator)
        }
    }

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

    impl JsonSchema for StrictString {
        fn inline_schema() -> bool {
            <String as JsonSchema>::inline_schema()
        }

        fn schema_name() -> Cow<'static, str> {
            <String as JsonSchema>::schema_name()
        }

        fn schema_id() -> Cow<'static, str> {
            <String as JsonSchema>::schema_id()
        }

        fn json_schema(generator: &mut SchemaGenerator) -> Schema {
            <String as JsonSchema>::json_schema(generator)
        }
    }

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

    impl JsonSchema for Severity {
        fn schema_name() -> Cow<'static, str> {
            "Severity".into()
        }

        fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
            const CANONICAL: [&str; 5] = ["debug", "info", "warn", "warning", "error"];
            const ASCII_CASE_INSENSITIVE: &str = concat!(
                r"^(",
                r"[dD][eE][bB][uU][gG]|",
                r"[iI][nN][fF][oO]|",
                r"[wW][aA][rR][nN]([iI][nN][gG])?|",
                r"[eE][rR][rR][oO][rR]",
                r")$"
            );

            schemars::json_schema!({
                "type": "string",
                "anyOf": [
                    { "enum": CANONICAL },
                    {
                        "description": "Non-lowercase severity spellings are deprecated; use lowercase.",
                        "allOf": [
                            { "pattern": ASCII_CASE_INSENSITIVE },
                            { "not": { "pattern": "[^A-Za-z]" } },
                            { "not": { "enum": CANONICAL } }
                        ]
                    }
                ]
            })
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

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[schemars(deny_unknown_fields)]
    struct RemoteRuleSchema {
        #[schemars(schema_with = "non_blank_no_nul_string_schema")]
        remote: StrictString,
    }

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[schemars(deny_unknown_fields)]
    struct ExecutableRuleSchema {
        #[serde(default)]
        name: StrictOptional<StrictString>,
        #[schemars(schema_with = "non_blank_no_nul_string_schema")]
        check: StrictString,
        #[serde(default)]
        severity: StrictOptional<Severity>,
        #[serde(default)]
        #[schemars(schema_with = "no_nul_string_schema")]
        fix: StrictOptional<StrictString>,
        #[serde(default)]
        hint: StrictOptional<StrictString>,
    }

    impl JsonSchema for Rule {
        fn schema_name() -> Cow<'static, str> {
            "Rule".into()
        }

        fn json_schema(generator: &mut SchemaGenerator) -> Schema {
            let remote = RemoteRuleSchema::json_schema(generator);
            let executable = ExecutableRuleSchema::json_schema(generator);

            schemars::json_schema!({
                "oneOf": [remote, executable]
            })
        }
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

    #[derive(Deserialize, JsonSchema)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct RawConfig {
        #[serde(default)]
        #[schemars(schema_with = "no_nul_string_schema")]
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
        #[schemars(schema_with = "patterns_schema")]
        patterns: Vec<StrictString>,
    }

    fn patterns_schema(generator: &mut SchemaGenerator) -> Schema {
        let items = pattern_string_schema(generator);
        schemars::json_schema!({
            "type": "array",
            "items": items
        })
    }

    impl JsonSchema for Config {
        fn schema_name() -> Cow<'static, str> {
            "checksy configuration".into()
        }

        fn json_schema(generator: &mut SchemaGenerator) -> Schema {
            RawConfig::json_schema(generator)
        }
    }

    pub(crate) fn configuration_schema() -> Schema {
        SchemaSettings::draft07()
            .into_generator()
            .into_root_schema_for::<Config>()
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
                if pattern.contains('\0') {
                    return Err(format!("patterns[{}] cannot contain a NUL byte", index));
                }
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::json;

        fn configuration_validator() -> jsonschema::Validator {
            let schema = serde_json::to_value(configuration_schema()).unwrap();
            jsonschema::draft7::meta::validate(&schema).unwrap();
            jsonschema::draft7::new(&schema).unwrap()
        }

        #[test]
        fn generated_schema_is_closed_optional_and_uses_the_exact_rule_union() {
            let schema = serde_json::to_value(configuration_schema()).unwrap();

            assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
            assert_eq!(schema["additionalProperties"], false);
            assert!(schema.get("required").is_none());

            let branches = schema["definitions"]["Rule"]["oneOf"]
                .as_array()
                .expect("Rule must be represented by oneOf");
            assert_eq!(branches.len(), 2);
            assert_eq!(branches[0]["additionalProperties"], false);
            assert_eq!(branches[0]["required"], json!(["remote"]));
            assert_eq!(branches[1]["additionalProperties"], false);
            assert_eq!(branches[1]["required"], json!(["check"]));
            assert!(branches[1]["properties"].get("remote").is_none());

            let severity = &schema["definitions"]["Severity"];
            assert!(severity.get("default").is_none());
            assert_eq!(
                severity["anyOf"][0]["enum"],
                json!(["debug", "info", "warn", "warning", "error"])
            );
        }

        #[test]
        fn generated_schema_enforces_runtime_structural_constraints() {
            let validator = configuration_validator();
            let accepted = [
                json!({}),
                json!({"cachePath": ""}),
                json!({"rules": [{"remote": "nested.yaml"}]}),
                json!({
                    "rules": [{
                        "name": "",
                        "check": "echo ok",
                        "severity": "warning",
                        "fix": "",
                        "hint": ""
                    }]
                }),
                json!({"rules": [{"check": "echo ok", "severity": "WaRnInG"}]}),
                json!({"patterns": ["[unterminated"]}),
            ];
            for instance in accepted {
                assert!(
                    validator.is_valid(&instance),
                    "schema unexpectedly rejected {instance}"
                );
            }

            let rejected = [
                json!({"cachePath": null}),
                json!({"checkSeverity": null}),
                json!({"rules": [{}]}),
                json!({"rules": [{"fix": "echo fixed"}]}),
                json!({"rules": [{"remote": "nested.yaml", "check": "echo mixed"}]}),
                json!({"rules": [{"remote": " \t\n"}]}),
                json!({"rules": [{"check": " \t\n"}]}),
                json!({"rules": [{"check": "\u{3000}"}]}),
                json!({"rules": [{"check": "echo ok", "name": null}]}),
                json!({"rules": [{"check": "echo ok", "severity": "critical"}]}),
                json!({"rules": [{"check": "echo ok", "severity": "ERROR\n"}]}),
                json!({"rules": [], "unknown": true}),
                json!({"patterns": [" ! \t"]}),
                json!({"patterns": ["!\u{3000}"]}),
                json!({"cachePath": "cache\0path"}),
                json!({"rules": [{"check": "echo\0ok"}]}),
                json!({"rules": [{"check": "false", "fix": "fix\0it"}]}),
                json!({"rules": [{"remote": "nested\0.yaml"}]}),
                json!({"patterns": ["scripts/\0*.sh"]}),
            ];
            for instance in rejected {
                assert!(
                    !validator.is_valid(&instance),
                    "schema unexpectedly accepted {instance}"
                );
            }
        }

        #[test]
        fn complete_glob_grammar_remains_a_runtime_validation_layer() {
            let validator = configuration_validator();
            assert!(validator.is_valid(&json!({"patterns": ["[unterminated"]})));
            assert!(
                serde_yaml::from_str::<Config>("rules: []\npatterns:\n  - '[unterminated'\n")
                    .is_err()
            );
        }
    }
}

pub use schema::*;
