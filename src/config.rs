use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

const CONFIG_FILE_NAME: &str = "control_freak.toml";

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub lints: LintsConfig,
}

impl Config {
    /// Load the closest `control_freak.toml` at or above `path`.
    pub fn load_for_path(path: &Path) -> Result<Self> {
        let start = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };

        for directory in start.ancestors() {
            let config_path = directory.join(CONFIG_FILE_NAME);
            if config_path.is_file() {
                return Self::load(&config_path);
            }
        }

        Ok(Self::default())
    }

    fn load(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read configuration: {}", path.display()))?;
        let config: Self = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse configuration: {}", path.display()))?;
        config.validate(path)?;
        Ok(config)
    }

    fn validate(&self, path: &Path) -> Result<()> {
        if self.lints.use_destructuring.min_fields == 0 {
            anyhow::bail!(
                "Invalid configuration in {}: lints.use_destructuring.min_fields must be at least 1",
                path.display(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LintsConfig {
    pub max_struct_fields: MaxStructFieldsConfig,
    pub max_visible_bindings: MaxVisibleBindingsConfig,
    pub negated_bool_names: LintConfig,
    pub use_destructuring: UseDestructuringConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LintConfig {
    pub enabled: bool,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MaxStructFieldsConfig {
    pub enabled: bool,
    pub max_fields: usize,
}

impl Default for MaxStructFieldsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_fields: 12,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MaxVisibleBindingsConfig {
    pub enabled: bool,
    pub max_bindings: usize,
}

impl Default for MaxVisibleBindingsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_bindings: 16,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UseDestructuringConfig {
    pub enabled: bool,
    pub min_fields: usize,
}

impl Default for UseDestructuringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_fields: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_enable_all_lints() {
        let config = Config::default();
        assert!(config.lints.max_struct_fields.enabled);
        assert_eq!(config.lints.max_struct_fields.max_fields, 12);
        assert!(config.lints.max_visible_bindings.enabled);
        assert_eq!(config.lints.max_visible_bindings.max_bindings, 16);
        assert!(config.lints.negated_bool_names.enabled);
        assert!(config.lints.use_destructuring.enabled);
        assert_eq!(config.lints.use_destructuring.min_fields, 3);
    }

    #[test]
    fn parses_lint_settings() {
        let config: Config = toml::from_str(
            r#"
            [lints.negated_bool_names]
            enabled = false

            [lints.use_destructuring]
            min_fields = 4
            "#,
        )
        .expect("valid configuration");

        assert!(!config.lints.negated_bool_names.enabled);
        assert!(config.lints.use_destructuring.enabled);
        assert_eq!(config.lints.use_destructuring.min_fields, 4);
    }

    #[test]
    fn finds_the_closest_configuration_file() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let project = temp_dir.path().join("project");
        let nested = project.join("src");
        fs::create_dir_all(&nested)?;
        fs::write(
            temp_dir.path().join(CONFIG_FILE_NAME),
            "[lints.use_destructuring]\nmin_fields = 4\n",
        )?;
        fs::write(
            project.join(CONFIG_FILE_NAME),
            "[lints.use_destructuring]\nmin_fields = 5\n",
        )?;

        let config = Config::load_for_path(&nested.join("main.rs"))?;

        assert_eq!(config.lints.use_destructuring.min_fields, 5);
        Ok(())
    }

    #[test]
    fn rejects_zero_minimum_fields() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let config_path = temp_dir.path().join(CONFIG_FILE_NAME);
        fs::write(&config_path, "[lints.use_destructuring]\nmin_fields = 0\n")
            .expect("write configuration");

        let err = Config::load_for_path(temp_dir.path()).expect_err("invalid configuration");

        assert!(err.to_string().contains("must be at least 1"));
    }
}
