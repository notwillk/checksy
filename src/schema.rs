pub mod schema {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
            let s = String::deserialize(deserializer)?;
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

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Rule {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        pub check: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub fix: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub hint: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct Config {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub check_severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub fail_severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub preconditions: Vec<Rule>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub rules: Vec<Rule>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub patterns: Vec<String>,
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
