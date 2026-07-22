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
    const RULE_TIMEOUT_PATTERN: &str = r"^[1-9][0-9]*(ms|s|m|h)$";

    pub(crate) const DEFAULT_COMMAND_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(15 * 60);
    pub(crate) const MAX_COMMAND_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(2 * 60 * 60);

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

    fn rule_timeout_schema(_generator: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": RULE_TIMEOUT_PATTERN
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

    /// An optional input field that distinguishes omission from explicit null.
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

    impl<T> StrictOptional<T> {
        fn into_option(self) -> Option<T> {
            self.0
        }
    }

    /// A YAML string that cannot be coerced from another scalar type or contain NUL.
    struct StrictString(String);

    impl<'de> Deserialize<'de> for StrictString {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            match serde_yaml::Value::deserialize(deserializer)? {
                serde_yaml::Value::String(value) if value.contains('\0') => Err(
                    serde::de::Error::custom("string values cannot contain a NUL byte"),
                ),
                serde_yaml::Value::String(value) => Ok(Self(value)),
                _ => Err(serde::de::Error::custom(
                    "invalid type: expected a YAML string",
                )),
            }
        }
    }

    impl JsonSchema for StrictString {
        fn inline_schema() -> bool {
            true
        }

        fn schema_name() -> Cow<'static, str> {
            "StrictString".into()
        }

        fn json_schema(generator: &mut SchemaGenerator) -> Schema {
            no_nul_string_schema(generator)
        }
    }

    impl StrictString {
        fn into_string(self) -> String {
            self.0
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
            let value = StrictString::deserialize(deserializer)?.into_string();
            match value.to_lowercase().as_str() {
                "error" => Ok(Severity::Error),
                "warn" | "warning" => Ok(Severity::Warning),
                "info" => Ok(Severity::Info),
                "debug" => Ok(Severity::Debug),
                _ => Err(serde::de::Error::custom(format!(
                    "unknown variant `{value}`, expected one of `error`, `warn`, `warning`, `info`, `debug`"
                ))),
            }
        }
    }

    impl JsonSchema for Severity {
        fn schema_name() -> Cow<'static, str> {
            "Severity".into()
        }

        fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
            const LOWERCASE: [&str; 5] = ["debug", "info", "warn", "warning", "error"];
            const ASCII_CASE_INSENSITIVE: &str = concat!(
                r"^(?:",
                r"[dD][eE][bB][uU][gG]|",
                r"[iI][nN][fF][oO]|",
                r"[wW][aA][rR][nN](?:[iI][nN][gG])?|",
                r"[eE][rR][rR][oO][rR]",
                r")$"
            );

            schemars::json_schema!({
                "type": "string",
                "anyOf": [
                    { "enum": LOWERCASE },
                    { "pattern": ASCII_CASE_INSENSITIVE }
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
        #[serde(rename = "interactive-fix", skip_serializing_if = "Option::is_none")]
        pub interactive_fix: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub hint: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub remote: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub timeout: Option<String>,
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
        #[serde(default, rename = "interactive-fix")]
        interactive_fix: StrictOptional<StrictString>,
        #[serde(default)]
        hint: StrictOptional<StrictString>,
        #[serde(default)]
        remote: StrictOptional<StrictString>,
        #[serde(default)]
        timeout: StrictOptional<StrictString>,
    }

    impl TryFrom<RawRule> for Rule {
        type Error = String;

        fn try_from(raw: RawRule) -> Result<Self, Self::Error> {
            let rule = Self {
                name: raw.name.into_option().map(StrictString::into_string),
                check: raw.check.into_option().map(StrictString::into_string),
                severity: raw.severity.into_option(),
                fix: raw.fix.into_option().map(StrictString::into_string),
                interactive_fix: raw
                    .interactive_fix
                    .into_option()
                    .map(StrictString::into_string),
                hint: raw.hint.into_option().map(StrictString::into_string),
                remote: raw.remote.into_option().map(StrictString::into_string),
                timeout: raw.timeout.into_option().map(StrictString::into_string),
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
            Rule::try_from(RawRule::deserialize(deserializer)?).map_err(serde::de::Error::custom)
        }
    }

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[schemars(deny_unknown_fields)]
    struct IncludeRuleSchema {
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
        fix: StrictOptional<StrictString>,
        #[serde(default, rename = "interactive-fix")]
        #[schemars(schema_with = "non_blank_no_nul_string_schema")]
        interactive_fix: StrictOptional<StrictString>,
        #[serde(default)]
        hint: StrictOptional<StrictString>,
        #[serde(default)]
        #[schemars(schema_with = "rule_timeout_schema")]
        timeout: StrictOptional<StrictString>,
    }

    impl JsonSchema for Rule {
        fn schema_name() -> Cow<'static, str> {
            "Rule".into()
        }

        fn json_schema(generator: &mut SchemaGenerator) -> Schema {
            let include = IncludeRuleSchema::json_schema(generator);
            let mut executable = ExecutableRuleSchema::json_schema(generator);
            executable
                .as_object_mut()
                .expect("derived executable rule schema must be an object")
                .insert(
                    "not".to_string(),
                    serde_json::json!({
                        "required": ["fix", "interactive-fix"]
                    }),
                );
            schemars::json_schema!({
                "oneOf": [include, executable]
            })
        }
    }

    impl Rule {
        fn validate(&self) -> Result<(), String> {
            self.validate_strings()?;

            if self.is_remote() {
                if let Some(error) = self.validate_remote_only() {
                    return Err(error);
                }
                if self.remote.as_deref().unwrap_or_default().trim().is_empty() {
                    return Err("include rule requires a non-empty `remote` value".to_string());
                }
                return Ok(());
            }

            let Some(check) = self.check.as_deref() else {
                if self.fix.is_some() {
                    return Err("executable rule cannot define `fix` without `check`".to_string());
                }
                if self.interactive_fix.is_some() {
                    return Err(
                        "executable rule cannot define `interactive-fix` without `check`"
                            .to_string(),
                    );
                }
                return Err(
                    "rule must contain exactly one of `remote` or a non-empty `check`".to_string(),
                );
            };

            if check.trim().is_empty() {
                return Err("executable rule requires a non-empty `check` value".to_string());
            }

            if self
                .interactive_fix
                .as_deref()
                .is_some_and(|command| command.trim().is_empty())
            {
                return Err(
                    "executable rule requires a non-empty `interactive-fix` value".to_string(),
                );
            }

            if self.fix.is_some() && self.interactive_fix.is_some() {
                return Err(
                    "executable rule cannot define both `fix` and `interactive-fix`".to_string(),
                );
            }

            if let Some(timeout) = self.timeout.as_deref() {
                parse_rule_timeout(timeout)?;
            }

            Ok(())
        }

        fn validate_strings(&self) -> Result<(), String> {
            for (field, value) in [
                ("name", self.name.as_deref()),
                ("check", self.check.as_deref()),
                ("fix", self.fix.as_deref()),
                ("interactive-fix", self.interactive_fix.as_deref()),
                ("hint", self.hint.as_deref()),
                ("remote", self.remote.as_deref()),
                ("timeout", self.timeout.as_deref()),
            ] {
                if value.is_some_and(|value| value.contains('\0')) {
                    return Err(format!("`{field}` cannot contain a NUL byte"));
                }
            }
            Ok(())
        }

        /// Returns true if this is an include rule (the legacy field is `remote`).
        pub fn is_remote(&self) -> bool {
            self.remote.is_some()
        }

        /// Validates that an include rule contains only its legacy `remote` field.
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
            if self.interactive_fix.is_some() {
                invalid_props.push("interactive-fix");
            }
            if self.hint.is_some() {
                invalid_props.push("hint");
            }
            if self.timeout.is_some() {
                invalid_props.push("timeout");
            }

            if invalid_props.is_empty() {
                None
            } else {
                Some(format!(
                    "remote rule cannot have properties: {}",
                    invalid_props.join(", ")
                ))
            }
        }

        pub(crate) fn effective_timeout(&self) -> Result<std::time::Duration, String> {
            self.timeout
                .as_deref()
                .map(parse_rule_timeout)
                .transpose()
                .map(|timeout| timeout.unwrap_or(DEFAULT_COMMAND_TIMEOUT))
        }
    }

    pub(crate) fn parse_rule_timeout(value: &str) -> Result<std::time::Duration, String> {
        let (digits, multiplier_ms) = if let Some(digits) = value.strip_suffix("ms") {
            (digits, 1_u64)
        } else if let Some(digits) = value.strip_suffix('s') {
            (digits, 1_000_u64)
        } else if let Some(digits) = value.strip_suffix('m') {
            (digits, 60_000_u64)
        } else if let Some(digits) = value.strip_suffix('h') {
            (digits, 3_600_000_u64)
        } else {
            return Err(format!(
                "timeout `{value}` must match {RULE_TIMEOUT_PATTERN}"
            ));
        };

        if digits.is_empty()
            || digits.starts_with('0')
            || !digits.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(format!(
                "timeout `{value}` must match {RULE_TIMEOUT_PATTERN}"
            ));
        }

        let number = digits
            .parse::<u64>()
            .map_err(|_| format!("timeout `{value}` numeric value is too large"))?;
        let milliseconds = number
            .checked_mul(multiplier_ms)
            .ok_or_else(|| format!("timeout `{value}` numeric value is too large"))?;
        let duration = std::time::Duration::from_millis(milliseconds);
        if duration > MAX_COMMAND_TIMEOUT {
            return Err(format!("timeout `{value}` must not exceed 2h"));
        }

        Ok(duration)
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

    impl Config {
        pub(crate) fn validate(&self) -> Result<(), String> {
            if self
                .cache_path
                .as_deref()
                .is_some_and(|path| path.contains('\0'))
            {
                return Err("`cachePath` cannot contain a NUL byte".to_string());
            }

            for (index, rule) in self.preconditions.iter().enumerate() {
                rule.validate()
                    .map_err(|error| format!("preconditions[{index}]: {error}"))?;
            }
            for (index, rule) in self.rules.iter().enumerate() {
                rule.validate()
                    .map_err(|error| format!("rules[{index}]: {error}"))?;
            }
            for (index, pattern) in self.patterns.iter().enumerate() {
                if pattern.contains('\0') {
                    return Err(format!("patterns[{index}] cannot contain a NUL byte"));
                }
                let pattern = pattern.trim();
                let pattern = pattern.strip_prefix('!').unwrap_or(pattern).trim();
                if pattern.is_empty() {
                    return Err(format!("patterns[{index}] must not be empty"));
                }
                glob::Pattern::new(pattern)
                    .map_err(|error| format!("patterns[{index}] is invalid: {error}"))?;
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

        #[test]
        fn strict_input_accepts_optional_rules_and_compatible_severities() {
            let empty: Config = serde_yaml::from_str("{}").unwrap();
            assert!(empty.rules.is_empty());

            for spelling in ["warn", "warning", "WaRn", "WARNING"] {
                let yaml = format!("rules:\n  - check: echo ok\n    severity: {spelling}\n");
                let config: Config = serde_yaml::from_str(&yaml).unwrap();
                assert_eq!(config.rules[0].severity, Some(Severity::Warning));
            }
        }

        #[test]
        fn public_configuration_types_still_round_trip_through_yaml_and_json() {
            let yaml = concat!(
                "rules:\n",
                "  - name: compatible\n",
                "    check: echo ok\n",
                "    severity: warning\n",
                "    timeout: 30s\n",
                "  - name: interactive\n",
                "    check: test -f ready\n",
                "    interactive-fix: read -r answer\n"
            );
            let config: Config = serde_yaml::from_str(yaml).unwrap();
            let reparsed_yaml: Config =
                serde_yaml::from_str(&serde_yaml::to_string(&config).unwrap()).unwrap();
            let reparsed_json: Config =
                serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();

            for reparsed in [reparsed_yaml, reparsed_json] {
                assert_eq!(reparsed.rules.len(), 2);
                assert_eq!(reparsed.rules[0].name.as_deref(), Some("compatible"));
                assert_eq!(reparsed.rules[0].check.as_deref(), Some("echo ok"));
                assert_eq!(reparsed.rules[0].severity, Some(Severity::Warning));
                assert_eq!(reparsed.rules[0].timeout.as_deref(), Some("30s"));
                assert_eq!(
                    reparsed.rules[1].interactive_fix.as_deref(),
                    Some("read -r answer")
                );
            }
        }

        #[test]
        fn rule_timeouts_are_strict_bounded_and_defaulted() {
            for (input, expected) in [
                ("1ms", std::time::Duration::from_millis(1)),
                ("1s", std::time::Duration::from_secs(1)),
                ("120m", MAX_COMMAND_TIMEOUT),
                ("2h", MAX_COMMAND_TIMEOUT),
                ("7200000ms", MAX_COMMAND_TIMEOUT),
            ] {
                assert_eq!(parse_rule_timeout(input).unwrap(), expected, "{input}");
            }

            for input in [
                "",
                "0ms",
                "01s",
                "1.5s",
                "1d",
                " 1s",
                "1s ",
                "3h",
                "121m",
                "7201s",
                "7200001ms",
                "18446744073709551616ms",
                "18446744073709551615h",
            ] {
                assert!(parse_rule_timeout(input).is_err(), "accepted {input:?}");
            }

            let default: Config = serde_yaml::from_str("rules:\n  - check: 'true'\n").unwrap();
            assert_eq!(
                default.rules[0].effective_timeout().unwrap(),
                DEFAULT_COMMAND_TIMEOUT
            );
            let explicit: Config =
                serde_yaml::from_str("rules:\n  - check: 'true'\n    timeout: 30s\n").unwrap();
            assert_eq!(
                explicit.rules[0].effective_timeout().unwrap(),
                std::time::Duration::from_secs(30)
            );
        }

        #[test]
        fn strict_input_rejects_unknown_null_coercion_and_invalid_rule_forms() {
            let rejected = [
                "unknown: true\n",
                "rules:\n  - check: ok\n    unknown: true\n",
                "rules: null\n",
                "rules:\n  - check: null\n",
                "rules:\n  - check: 123\n",
                "rules:\n  - {}\n",
                "rules:\n  - check: '  '\n",
                "rules:\n  - remote: '  '\n",
                "rules:\n  - remote: nested.yaml\n    check: echo mixed\n",
                "rules:\n  - remote: nested.yaml\n    timeout: 1s\n",
                "rules:\n  - fix: echo fixed\n",
                "rules:\n  - interactive-fix: read -r answer\n",
                "rules:\n  - check: ok\n    interactive-fix: '  '\n",
                "rules:\n  - check: ok\n    interactive-fix: null\n",
                "rules:\n  - check: ok\n    interactive-fix: 1\n",
                "rules:\n  - check: ok\n    fix: echo fixed\n    interactive-fix: read -r answer\n",
                "rules:\n  - remote: nested.yaml\n    interactive-fix: read -r answer\n",
                "rules:\n  - check: ok\n    timeout: null\n",
                "rules:\n  - check: ok\n    timeout: 1\n",
                "rules:\n  - check: ok\n    timeout: 0s\n",
                "rules:\n  - check: ok\n    timeout: 3h\n",
            ];

            for yaml in rejected {
                assert!(
                    serde_yaml::from_str::<Config>(yaml).is_err(),
                    "unexpectedly accepted {yaml:?}"
                );
            }
        }

        #[test]
        fn strict_input_rejects_nul_in_every_string_field() {
            let rejected = [
                "cachePath: \"cache\\0path\"\n",
                "checkSeverity: \"error\\0\"\n",
                "rules:\n  - name: \"bad\\0name\"\n    check: ok\n",
                "rules:\n  - check: \"bad\\0check\"\n",
                "rules:\n  - check: 'false'\n    fix: \"bad\\0fix\"\n",
                "rules:\n  - check: 'false'\n    interactive-fix: \"bad\\0interactive\"\n",
                "rules:\n  - check: 'false'\n    hint: \"bad\\0hint\"\n",
                "rules:\n  - remote: \"bad\\0remote\"\n",
                "rules:\n  - check: ok\n    timeout: \"bad\\0timeout\"\n",
                "patterns:\n  - \"bad\\0pattern\"\n",
            ];

            for yaml in rejected {
                let error = serde_yaml::from_str::<Config>(yaml)
                    .unwrap_err()
                    .to_string();
                assert!(error.contains("NUL byte"), "unexpected error: {error}");
            }
        }

        #[test]
        fn complete_glob_grammar_is_enforced_at_runtime() {
            assert!(serde_yaml::from_str::<Config>("patterns:\n  - ''\n").is_err());
            assert!(serde_yaml::from_str::<Config>("patterns:\n  - '!   '\n").is_err());
            assert!(serde_yaml::from_str::<Config>("patterns:\n  - '[unterminated'\n").is_err());
            assert!(serde_yaml::from_str::<Config>("patterns:\n  - 'scripts/*.sh'\n").is_ok());
        }

        #[test]
        fn generated_schema_is_closed_optional_and_uses_an_exact_rule_union() {
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
            assert!(schema["definitions"]["Severity"].get("default").is_none());
            assert!(branches[1]["properties"]["severity"]
                .get("default")
                .is_none());
            assert_eq!(
                branches[1]["properties"]["timeout"]["pattern"],
                RULE_TIMEOUT_PATTERN
            );
            assert!(branches[1]["properties"].get("interactive-fix").is_some());
            assert!(branches[1]["properties"].get("interactive_fix").is_none());
            assert_eq!(
                branches[1]["not"]["required"],
                json!(["fix", "interactive-fix"])
            );
            assert!(branches[1]["properties"]["timeout"]
                .get("default")
                .is_none());
            assert!(branches[0]["properties"].get("timeout").is_none());
            assert!(branches[0]["properties"].get("interactive-fix").is_none());
        }

        #[test]
        fn generated_schema_matches_representative_structural_constraints() {
            let schema = serde_json::to_value(configuration_schema()).unwrap();
            jsonschema::draft7::meta::validate(&schema).unwrap();
            let validator = jsonschema::draft7::new(&schema).unwrap();

            for accepted in [
                json!({}),
                json!({"rules": [{"check": "echo ok", "severity": "WaRnInG"}]}),
                json!({"rules": [{"remote": "nested.yaml"}]}),
                json!({"rules": [{"check": "false", "interactive-fix": "read -r answer"}]}),
                json!({"rules": [{"check": "true", "timeout": "1ms"}]}),
                json!({"rules": [{"check": "true", "timeout": "2h"}]}),
                json!({"patterns": ["scripts/*.sh"]}),
            ] {
                assert!(validator.is_valid(&accepted), "rejected {accepted}");
            }

            for rejected in [
                json!({"unknown": true}),
                json!({"rules": null}),
                json!({"rules": [{}]}),
                json!({"rules": [{"check": " "}]}),
                json!({"rules": [{"remote": "nested.yaml", "check": "echo mixed"}]}),
                json!({"rules": [{"remote": "nested.yaml", "timeout": "1s"}]}),
                json!({"rules": [{"remote": "nested.yaml", "interactive-fix": "read -r answer"}]}),
                json!({"rules": [{"interactive-fix": "read -r answer"}]}),
                json!({"rules": [{"check": "false", "interactive-fix": ""}]}),
                json!({"rules": [{"check": "false", "interactive-fix": "  "}]}),
                json!({"rules": [{"check": "false", "interactive-fix": null}]}),
                json!({"rules": [{"check": "false", "interactive-fix": 1}]}),
                json!({"rules": [{"check": "false", "interactive-fix": "bad\0command"}]}),
                json!({"rules": [{"check": "false", "fix": "true", "interactive-fix": "read -r answer"}]}),
                json!({"rules": [{"check": "ok", "name": "bad\0name"}]}),
                json!({"rules": [{"check": "ok", "timeout": null}]}),
                json!({"rules": [{"check": "ok", "timeout": 1}]}),
                json!({"rules": [{"check": "ok", "timeout": "0s"}]}),
                json!({"rules": [{"check": "ok", "timeout": "1.5s"}]}),
                json!({"rules": [{"check": "ok", "timeout": "1d"}]}),
                json!({"rules": [{"check": "ok", "timeout": "bad\0timeout"}]}),
                json!({"patterns": ["!  "]}),
            ] {
                assert!(!validator.is_valid(&rejected), "accepted {rejected}");
            }

            assert!(validator.is_valid(&json!({"patterns": ["[unterminated"]})));
            assert!(validator.is_valid(&json!({
                "rules": [{"check": "true", "timeout": "3h"}]
            })));
            assert!(validator.is_valid(&json!({
                "rules": [{"check": "true", "timeout": "18446744073709551616ms"}]
            })));
        }
    }
}

pub use schema::*;
