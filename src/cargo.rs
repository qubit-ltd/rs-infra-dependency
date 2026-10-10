// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Cargo metadata loading and resolved-package projection.

use std::process::Command;

use camino::Utf8Path;
use camino::Utf8PathBuf;
use cargo_metadata::Metadata;
use cargo_metadata::MetadataCommand;
use cargo_metadata::Package;
use serde::Deserialize;
use serde::Serialize;

use crate::PolicyError;

/// A resolved package represented in a policy report.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::ResolvedPackage;
///
/// let package = ResolvedPackage {
///     name: "serde".into(),
///     version: semver::Version::parse("1.0.0").expect("valid version"),
///     source: Some("registry+https://github.com/rust-lang/crates.io-index".into()),
/// };
/// assert_eq!(package.name, "serde");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedPackage {
    /// Cargo package name used in reports and diagnostics.
    pub name: String,
    /// Version selected by Cargo for this package.
    pub version: semver::Version,
    /// Registry, Git, or path source identifier, when Cargo reports one.
    pub source: Option<String>,
}

#[derive(Deserialize)]
struct ToolDefaults {
    build_toolchain: String,
}

/// Selects the project build toolchain for Cargo subprocesses.
///
/// # Errors
///
/// Returns a configuration error with the selected path when the defaults
/// file is missing, unreadable, invalid, or has no nonempty build toolchain.
pub(crate) fn build_toolchain(project: &Utf8Path) -> Result<String, PolicyError> {
    let path = project.join(".infra/tools/defaults.toml");
    let source = std::fs::read_to_string(path.as_std_path()).map_err(|source| PolicyError::ReadConfig {
        path: path.to_string(),
        source,
    })?;
    let defaults: ToolDefaults = toml::from_str(&source).map_err(|source| PolicyError::ParseConfig {
        path: path.to_string(),
        source,
    })?;
    if defaults.build_toolchain.trim().is_empty() {
        return Err(PolicyError::InvalidConfig {
            code: "DP001",
            message: format!("build_toolchain must not be empty in {path}"),
        });
    }
    Ok(defaults.build_toolchain)
}

/// Loads root-package Cargo metadata without resolving or updating
/// dependencies.
pub(crate) fn load_metadata(project: &Utf8Path) -> Result<(Metadata, Option<Package>), PolicyError> {
    let toolchain = build_toolchain(project)?;
    let manifest = project.join("Cargo.toml");
    let mut command = MetadataCommand::new();
    command.manifest_path(manifest.as_std_path());
    command.no_deps();
    command.env("RUSTUP_TOOLCHAIN", toolchain);
    let metadata = command.exec().map_err(|error| PolicyError::Cargo {
        code: "DP201",
        message: error.to_string(),
    })?;
    let root = metadata.root_package().cloned();
    Ok((metadata, root))
}

/// Returns the Cargo workspace root without resolving dependencies.
pub(crate) fn workspace_root(project: &Utf8Path, toolchain: &str) -> Result<Utf8PathBuf, PolicyError> {
    let manifest = project.join("Cargo.toml");
    let output = Command::new("cargo")
        .env("RUSTUP_TOOLCHAIN", toolchain)
        .args([
            "locate-project",
            "--workspace",
            "--message-format",
            "plain",
            "--manifest-path",
            manifest.as_str(),
        ])
        .output();
    workspace_root_from_output(output)
}

fn workspace_root_from_output(output: std::io::Result<std::process::Output>) -> Result<Utf8PathBuf, PolicyError> {
    let output = output.map_err(|error| PolicyError::Cargo {
        code: "DP201",
        message: format!("failed to locate Cargo workspace: {error}"),
    })?;
    if !output.status.success() {
        return Err(PolicyError::Cargo {
            code: "DP201",
            message: String::from_utf8_lossy(&output.stderr).trim().into(),
        });
    }
    let manifest = String::from_utf8(output.stdout).map_err(|error| PolicyError::Cargo {
        code: "DP201",
        message: format!("Cargo workspace path is not UTF-8: {error}"),
    })?;
    let manifest = Utf8PathBuf::from(manifest.trim());
    manifest
        .parent()
        .map(Utf8Path::to_owned)
        .ok_or_else(|| PolicyError::Cargo {
            code: "DP201",
            message: "Cargo workspace manifest has no parent".into(),
        })
}

/// Loads the complete resolved graph while refusing to change Cargo.lock.
pub(crate) fn load_locked_metadata(project: &Utf8Path) -> Result<Metadata, PolicyError> {
    let toolchain = build_toolchain(project)?;
    let manifest = project.join("Cargo.toml");
    let mut command = MetadataCommand::new();
    command.manifest_path(manifest.as_std_path());
    command.other_options(vec!["--locked".into(), "--all-features".into()]);
    command.env("RUSTUP_TOOLCHAIN", toolchain);
    command.exec().map_err(|error| PolicyError::Cargo {
        code: "DP404",
        message: format!("locked Cargo metadata failed: {error}"),
    })
}

/// Converts Cargo metadata packages into stable report records.
pub(crate) fn resolved_packages(metadata: &Metadata) -> Vec<ResolvedPackage> {
    metadata
        .packages
        .iter()
        .map(|package| ResolvedPackage {
            name: package.name.to_string(),
            version: package.version.clone(),
            source: package.source.as_ref().map(ToString::to_string),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use camino::Utf8Path;
    use camino::Utf8PathBuf;

    use super::build_toolchain;
    use super::load_locked_metadata;
    use super::load_metadata;
    use super::resolved_packages;
    use super::workspace_root;
    use super::workspace_root_from_output;

    fn project() -> (tempfile::TempDir, Utf8PathBuf) {
        let temporary = tempfile::tempdir().expect("temporary project");
        let root = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 project path");
        std::fs::create_dir_all(root.join(".infra/tools")).expect("tool defaults directory");
        std::fs::write(
            root.join(".infra/tools/defaults.toml"),
            "build_toolchain = \"1.94.0\"\n",
        )
        .expect("tool defaults");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"coverage-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("manifest");
        std::fs::create_dir_all(root.join("src")).expect("source directory");
        std::fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n").expect("source");
        std::fs::write(
            root.join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"coverage-fixture\"\nversion = \"0.1.0\"\n",
        )
        .expect("lockfile");
        (temporary, root)
    }

    #[test]
    fn reads_toolchain_and_reports_bad_defaults() {
        let (_temporary, root) = project();
        assert_eq!(build_toolchain(&root).unwrap(), "1.94.0");
        std::fs::write(root.join(".infra/tools/defaults.toml"), "build_toolchain = \" \"\n").unwrap();
        assert!(build_toolchain(&root).is_err());
        std::fs::write(root.join(".infra/tools/defaults.toml"), "not = [toml").unwrap();
        assert!(build_toolchain(&root).is_err());
        assert!(build_toolchain(Utf8Path::new("missing-project")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn maps_workspace_process_and_output_failures() {
        use std::os::unix::process::ExitStatusExt;

        let start_error = workspace_root_from_output(Err(std::io::Error::other("missing cargo")))
            .expect_err("process start error should be reported");
        assert!(start_error.to_string().contains("failed to locate Cargo workspace"));

        let invalid_utf8 = workspace_root_from_output(Ok(std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: vec![0xff],
            stderr: Vec::new(),
        }))
        .expect_err("non-UTF-8 workspace path should be reported");
        assert!(invalid_utf8.to_string().contains("not UTF-8"));

        let missing_parent = workspace_root_from_output(Ok(std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }))
        .expect_err("manifest without a parent should be rejected");
        assert!(missing_parent.to_string().contains("has no parent"));
    }

    #[test]
    fn loads_project_metadata_and_workspace_root() {
        let (_temporary, root) = project();
        let (metadata, package) = load_metadata(&root).expect("metadata");
        assert_eq!(package.unwrap().name, "coverage-fixture");
        assert_eq!(metadata.root_package().unwrap().name, "coverage-fixture");
        assert_eq!(workspace_root(&root, "1.94.0").unwrap(), root);
        let locked = load_locked_metadata(&root).expect("locked metadata");
        assert_eq!(resolved_packages(&locked).len(), 1);
        assert!(load_metadata(Utf8Path::new("missing-project")).is_err());
        assert!(workspace_root(Utf8Path::new("missing-project"), "1.94.0").is_err());
        assert!(load_locked_metadata(Utf8Path::new("missing-project")).is_err());

        std::fs::write(root.join("Cargo.lock"), "not valid lockfile").expect("invalid lockfile");
        assert!(load_locked_metadata(&root).is_err());
        std::fs::write(root.join("Cargo.lock"), "version = 4\n").expect("restore lockfile");
        std::fs::write(root.join("Cargo.toml"), "not = [valid toml").expect("invalid manifest");
        assert!(load_metadata(&root).is_err());
        assert!(workspace_root(&root, "1.94.0").is_err());
    }

    #[test]
    fn classifies_registry_prefix_and_path_dependencies() {
        let (_temporary, root) = project();
        std::fs::create_dir_all(root.join("local/src")).expect("local dependency");
        std::fs::write(
            root.join("local/Cargo.toml"),
            "[package]\nname = \"local-dependency\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("local manifest");
        std::fs::write(root.join("local/src/lib.rs"), "pub fn local() {}\n").expect("local source");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"coverage-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\nserde = \"1.0\"\nlocal-dependency = { path = \"local\" }\n",
        )
        .expect("manifest with dependencies");
        let (_, package) = load_metadata(&root).expect("metadata");
        let package = package.expect("root package");
        let config = crate::ProjectConfig {
            format: 2,
            source: String::new(),
            revision: String::new(),
            baseline: "v1".into(),
            internal_prefixes: vec!["ser".into()],
        };
        let serde = package
            .dependencies
            .iter()
            .find(|dependency| dependency.name == "serde")
            .unwrap();
        let local = package
            .dependencies
            .iter()
            .find(|dependency| dependency.name == "local-dependency")
            .unwrap();
        assert!(config.is_internal_dependency(serde));
        assert!(config.is_internal_dependency(local));

        let public_config = crate::ProjectConfig {
            internal_prefixes: Vec::new(),
            ..config
        };
        assert!(!public_config.is_internal_dependency(serde));
    }
}
