// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Cargo metadata loading and resolved-package projection.

use camino::Utf8Path;
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
    /// Package name.
    pub name: String,
    /// Resolved package version.
    pub version: semver::Version,
    /// Registry or Git source, if any.
    pub source: Option<String>,
}

/// Loads root-package Cargo metadata without resolving or updating dependencies.
pub(crate) fn load_metadata(
    project: &Utf8Path,
) -> Result<(Metadata, Option<Package>), PolicyError> {
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
