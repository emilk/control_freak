use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// The name of the configuration file `control_freak` looks for.
pub const CONFIG_FILE_NAME: &str = "control_freak.toml";

/// Contents of `control_freak.toml`.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Skip files marked `linguist-generated` in `.gitattributes` when walking directories.
    pub skip_generated: bool,

    /// Don't lint test code: `#[test]` functions, `#[cfg(test)]` items, and integration tests.
    pub skip_tests: bool,

    /// Settings for each lint.
    pub lints: LintsConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            skip_generated: true,
            skip_tests: true,
            lints: LintsConfig::default(),
        }
    }
}

impl Config {
    /// Load the closest `control_freak.toml` at or above `path`.
    ///
    /// # Errors
    /// If the configuration file can't be read, parsed, or is invalid.
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

    /// Load the configuration file at `path`.
    ///
    /// # Errors
    /// If the configuration file can't be read, parsed, or is invalid.
    pub fn load(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read configuration: {}", path.display()))?;
        let config: Self = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse configuration: {}", path.display()))?;
        config.validate(path)?;
        Ok(config)
    }

    /// The default configuration as TOML, with a comment describing each lint.
    ///
    /// # Errors
    /// If the configuration can't be serialized.
    pub fn default_toml() -> Result<String> {
        let toml =
            toml::to_string(&Self::default()).context("Failed to serialize configuration")?;
        let mut text = format!(
            "# Configuration for control_freak: {}\n# These are the defaults.\n\n",
            env!("CARGO_PKG_REPOSITORY"),
        );
        for line in toml.lines() {
            if line.starts_with("skip_generated =") {
                text += "# Skip files marked `linguist-generated` in `.gitattributes`\n";
            } else if line.starts_with("skip_tests =") {
                text += "# Don't lint `#[test]` functions, `#[cfg(test)]` items, and integration tests\n";
            }
            if let Some(name) = line
                .strip_prefix("[lints.")
                .and_then(|rest| rest.strip_suffix(']'))
                && let Some(lint) = crate::lints::LINTS.iter().find(|lint| lint.name == name)
            {
                writeln!(text, "# {}", lint.summary).ok();
            }
            text += line;
            text += "\n";
        }
        Ok(text)
    }

    fn validate(&self, path: &Path) -> Result<()> {
        let use_destructuring = &self.lints.use_destructuring;
        if use_destructuring.min_fields == 0 || use_destructuring.owned.min_fields == 0 {
            anyhow::bail!(
                "Invalid configuration in {}: lints.use_destructuring min_fields must be at least 1",
                path.display(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LintsConfig {
    pub dyn_any_params: LintConfig,
    pub max_mut_bindings: MaxMutBindingsConfig,
    pub max_struct_fields: MaxStructFieldsConfig,
    pub max_visible_bindings: MaxVisibleBindingsConfig,
    pub negated_bool_names: LintConfig,
    pub use_destructuring: UseDestructuringConfig,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LintConfig {
    pub enabled: bool,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MaxMutBindingsConfig {
    pub enabled: bool,
    pub max_bindings: usize,
}

impl Default for MaxMutBindingsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_bindings: 6,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MaxStructFieldsConfig {
    pub enabled: bool,
    pub max_fields: usize,
}

impl Default for MaxStructFieldsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_fields: 16,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MaxVisibleBindingsConfig {
    pub enabled: bool,
    pub max_bindings: usize,
}

impl Default for MaxVisibleBindingsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_bindings: 24,
        }
    }
}

/// Settings for the `use_destructuring` lint.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UseDestructuringConfig {
    /// Is the lint enabled?
    pub enabled: bool,

    /// Borrowed values (`&self`, `foo: &Foo`) with at least this many fields
    /// trigger when every field is accessed individually.
    pub min_fields: usize,

    /// Settings for owned values (`self`, `foo: Foo`).
    pub owned: OwnedDestructuringConfig,
}

impl Default for UseDestructuringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_fields: 3,
            owned: OwnedDestructuringConfig::default(),
        }
    }
}

/// Settings for owned values in the `use_destructuring` lint.
///
/// Fields of an owned value that are never accessed are silently dropped,
/// which is often a bug, so these also trigger when a few fields are missing.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OwnedDestructuringConfig {
    /// Only structs with at least this many fields trigger.
    pub min_fields: usize,

    /// Also trigger if at most this many fields are never accessed.
    pub max_missing_fields: usize,
}

impl Default for OwnedDestructuringConfig {
    fn default() -> Self {
        Self {
            min_fields: 2,
            max_missing_fields: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_enable_all_lints() {
        let config = Config::default();
        assert!(config.skip_generated);
        assert!(config.skip_tests);
        assert!(config.lints.dyn_any_params.enabled);
        assert!(config.lints.max_mut_bindings.enabled);
        assert_eq!(config.lints.max_mut_bindings.max_bindings, 6);
        assert!(config.lints.max_struct_fields.enabled);
        assert_eq!(config.lints.max_struct_fields.max_fields, 16);
        assert!(config.lints.max_visible_bindings.enabled);
        assert_eq!(config.lints.max_visible_bindings.max_bindings, 24);
        assert!(config.lints.negated_bool_names.enabled);
        assert!(config.lints.use_destructuring.enabled);
        assert_eq!(config.lints.use_destructuring.min_fields, 3);
        assert_eq!(config.lints.use_destructuring.owned.min_fields, 2);
        assert_eq!(config.lints.use_destructuring.owned.max_missing_fields, 1);
    }

    #[test]
    fn default_toml_round_trips_and_documents_every_lint() -> Result<()> {
        let text = Config::default_toml()?;
        let parsed: Config = toml::from_str(&text)?;
        assert_eq!(
            toml::to_string(&parsed)?,
            toml::to_string(&Config::default())?
        );
        for lint in crate::lints::LINTS {
            assert!(text.contains(&format!("# {}\n[lints.{}]", lint.summary, lint.name)));
        }
        Ok(())
    }

    #[test]
    fn every_lint_has_a_config_section() {
        for lint in crate::lints::LINTS {
            let toml = format!("[lints.{}]\nenabled = false\n", lint.name);
            assert!(toml::from_str::<Config>(&toml).is_ok(), "{}", lint.name);
        }
    }

    #[test]
    fn parses_lint_settings() {
        let config: Config = toml::from_str(
            r"
            [lints.negated_bool_names]
            enabled = false

            [lints.use_destructuring]
            min_fields = 4
            ",
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
