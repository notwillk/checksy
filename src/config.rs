use crate::schema::{Config, Severity};
use std::fs;
use std::path::Path;

pub fn resolve_path(explicit: &str) -> Result<Option<String>, String> {
    if !explicit.is_empty() {
        let path = Path::new(explicit);
        if !path.exists() {
            return Err(format!("config file {} does not exist", explicit));
        }
        if path.is_dir() {
            return Err(format!("config file {} is a directory", explicit));
        }
        return Ok(Some(explicit.to_string()));
    }

    for candidate in &[".checksy.yaml", ".checksy.yml"] {
        let path = Path::new(candidate);
        if path.exists() {
            if path.is_dir() {
                return Err(format!("config file {} is a directory", candidate));
            }
            return Ok(Some(candidate.to_string()));
        }
    }

    Ok(None)
}

pub fn load(path: &str) -> Result<Config, String> {
    let data = fs::read_to_string(path).map_err(|e| format!("read config: {}", e))?;

    let json_data = serde_yaml::from_str::<serde_yaml::Value>(&data)
        .map_err(|e| format!("decode config YAML: {}", e))?;

    let _json_str =
        serde_json::to_string(&json_data).map_err(|e| format!("convert config to JSON: {}", e))?;

    let mut cfg: Config =
        serde_yaml::from_str(&data).map_err(|e| format!("decode config: {}", e))?;

    apply_rule_defaults(&mut cfg);

    Ok(cfg)
}

fn apply_rule_defaults(cfg: &mut Config) {
    for rule in &mut cfg.rules {
        if rule.severity.is_none() {
            rule.severity = Some(Severity::Error);
        }
    }

    for rule in &mut cfg.preconditions {
        if rule.severity.is_none() {
            rule.severity = Some(Severity::Error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Rule;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_path_explicit() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cfg.yaml");
        fs::write(&path, "rules: []").unwrap();

        let got = resolve_path(path.to_str().unwrap());
        assert!(got.is_ok());
        assert!(got.unwrap().is_some());
    }

    #[test]
    fn test_resolve_path_auto_detect() {
        let dir = TempDir::new().unwrap();
        let old_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        fs::write(
            dir.path().join(".checksy.yaml"),
            "rules:\n  - check: echo ok\n",
        )
        .unwrap();

        let got = resolve_path("");
        std::env::set_current_dir(old_cwd).unwrap();

        assert!(got.is_ok());
        assert_eq!(got.unwrap(), Some(".checksy.yaml".to_string()));
    }

    #[test]
    fn test_apply_rule_defaults() {
        let mut cfg = Config {
            check_severity: None,
            fail_severity: None,
            preconditions: vec![],
            rules: vec![
                Rule {
                    name: None,
                    check: "echo hi".to_string(),
                    severity: None,
                    fix: None,
                    hint: None,
                },
                Rule {
                    name: None,
                    check: "echo warn".to_string(),
                    severity: Some(Severity::Warning),
                    fix: None,
                    hint: None,
                },
            ],
            patterns: vec![],
        };

        apply_rule_defaults(&mut cfg);

        assert_eq!(cfg.rules[0].severity, Some(Severity::Error));
        assert_eq!(cfg.rules[1].severity, Some(Severity::Warning));
    }

    #[test]
    fn test_load_applies_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(&path, "rules:\n  - name: warn\n    check: echo warn\n    severity: warn\n  - name: default\n    check: echo ok\n").unwrap();

        let result = load(path.to_str().unwrap());
        if let Err(e) = &result {
            eprintln!("Load error: {}", e);
        }
        assert!(result.is_ok(), "Failed to load config");

        let cfg = result.unwrap();
        assert_eq!(cfg.rules[0].severity, Some(Severity::Warning));
        assert_eq!(cfg.rules[1].severity, Some(Severity::Error));
    }

    #[test]
    fn test_load_patterns() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            "rules: []\npatterns:\n  - 'tests/*.sh'\n  - '!tests/skip.sh'\n",
        )
        .unwrap();

        let cfg = load(path.to_str().unwrap());
        assert!(cfg.is_ok());

        let cfg = cfg.unwrap();
        assert_eq!(cfg.patterns.len(), 2);
        assert_eq!(cfg.patterns[0], "tests/*.sh");
    }
}
