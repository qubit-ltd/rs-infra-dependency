// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Project policy pointer parsing and validation.

use camino::Utf8Path;
use serde::Deserialize;

use crate::PolicyError;

/// Project-owned policy selection loaded from `.infra/dependency/policy.toml`.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::ProjectConfig;
///
/// let config = ProjectConfig {
///     format: 2,
///     source: String::new(),
///     revision: String::new(),
///     baseline: "v2026.09.0".into(),
///     internal_prefixes: vec!["acme-".into()],
/// };
/// assert_eq!(config.format, 2);
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    /// Configuration schema version; currently this must be `2`.
    pub format: u32,
    /// Optional legacy source URL; new projects use the dynamically cached source.
    #[serde(default)]
    pub source: String,
    /// Optional legacy Git commit SHA selecting immutable policy contents.
    #[serde(default)]
    pub revision: String,
    /// Name of the selected baseline file without its `.txt` suffix.
    pub baseline: String,
    /// Organization-defined prefixes identifying first-party published crates.
    #[serde(default, rename = "internal-prefixes")]
    pub internal_prefixes: Vec<String>,
}

impl ProjectConfig {
    /// Loads and validates the project policy configuration.
    ///
    /// When `override_path` is `None`, this reads
    /// `.infra/dependency/policy.toml` below `project_root`; otherwise it reads
    /// the explicitly supplied path.
    ///
    /// # Errors
    ///
    /// Returns a [`PolicyError`] when the file cannot be read, its TOML is
    /// invalid, or the parsed values violate the configuration contract.
    ///
    /// # Parameters
    ///
    /// * `project_root` - Project directory containing the default policy path.
    /// * `override_path` - Optional explicit configuration path.
    ///
    /// # Returns
    ///
    /// Returns the validated project configuration.
    pub fn load(project_root: &Utf8Path, override_path: Option<&Utf8Path>) -> Result<Self, PolicyError> {
        let path = override_path
            .map(Utf8Path::to_owned)
            .unwrap_or_else(|| project_root.join(".infra/dependency/policy.toml"));
        let text = std::fs::read_to_string(path.as_std_path()).map_err(|source| PolicyError::ReadConfig {
            path: path.to_string(),
            source,
        })?;
        let config: Self = toml::from_str(&text).map_err(|source| PolicyError::ParseConfig {
            path: path.to_string(),
            source,
        })?;
        config.validate()
    }

    /// Returns whether a Cargo metadata dependency is first-party.
    ///
    /// Path dependencies, workspace dependencies, and names matching one of
    /// the configured internal prefixes are treated as first-party.
    ///
    /// # Parameters
    ///
    /// * `dependency` - Cargo metadata dependency to classify.
    ///
    /// # Returns
    ///
    /// Returns `true` when the dependency is treated as first-party.
    #[must_use]
    #[inline]
    pub fn is_internal_dependency(&self, dependency: &cargo_metadata::Dependency) -> bool {
        dependency.path.is_some()
            || dependency.source.is_none()
            || self
                .internal_prefixes
                .iter()
                .any(|prefix| dependency.name.starts_with(prefix))
    }

    /// Validates the schema version, release name, and optional legacy revision.
    fn validate(self) -> Result<Self, PolicyError> {
        if self.format != 2 {
            return Err(PolicyError::InvalidConfig {
                code: "DP001",
                message: format!("unsupported configuration format {}", self.format),
            });
        }
        if self.baseline.is_empty() || self.baseline.contains('/') || self.baseline.contains('\\') {
            return Err(PolicyError::InvalidConfig {
                code: "DP001",
                message: "baseline must be a simple release name".into(),
            });
        }
        if !self.revision.is_empty()
            && (self.revision.len() != 40 || !self.revision.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(PolicyError::InvalidConfig {
                code: "DP001",
                message: "revision must be a 40-digit hexadecimal commit SHA".into(),
            });
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectConfig;

    #[test]
    fn project_policy_can_select_a_baseline_without_pinning_its_source() {
        let config: ProjectConfig = toml::from_str(
            "format = 2\nbaseline = \"v2026.09.14\"\ninternal-prefixes = [\"qubit-\"]\n",
        )
        .unwrap();

        let config = config.validate().unwrap();
        assert_eq!(config.baseline, "v2026.09.14");
        assert!(config.source.is_empty());
        assert!(config.revision.is_empty());
    }
}
