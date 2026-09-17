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
use serde::Serialize;

use crate::PolicyError;

// qubit-style: allow type-file-name

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

/// Loads root-package Cargo metadata without resolving or updating
/// dependencies.
pub(crate) fn load_metadata(project: &Utf8Path) -> Result<(Metadata, Option<Package>), PolicyError> {
    let manifest = project.join("Cargo.toml");
    let mut command = MetadataCommand::new();
    command.manifest_path(manifest.as_std_path());
    command.no_deps();
    let metadata = command.exec().map_err(|error| PolicyError::Cargo {
        code: "DP201",
        message: error.to_string(),
    })?;
    let root = metadata.root_package().cloned();
    Ok((metadata, root))
}

/// Returns the Cargo workspace root without resolving dependencies.
pub(crate) fn workspace_root(project: &Utf8Path) -> Result<Utf8PathBuf, PolicyError> {
    let manifest = project.join("Cargo.toml");
    let output = Command::new("cargo")
        .args([
            "locate-project",
            "--workspace",
            "--message-format",
            "plain",
            "--manifest-path",
            manifest.as_str(),
        ])
        .output()
        .map_err(|error| PolicyError::Cargo {
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
    let manifest = project.join("Cargo.toml");
    let mut command = MetadataCommand::new();
    command.manifest_path(manifest.as_std_path());
    command.other_options(vec!["--locked".into(), "--all-features".into()]);
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
