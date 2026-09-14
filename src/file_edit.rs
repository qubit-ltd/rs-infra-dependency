// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Manifest edits planned by dependency synchronization.

/// A planned replacement in a manifest.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::FileEdit;
///
/// let edit = FileEdit {
///     path: "Cargo.toml".into(),
///     dependency: "serde".into(),
///     old: "1.0".into(),
///     new: "1.0".into(),
/// };
/// assert_eq!(edit.dependency, "serde");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEdit {
    /// Manifest path containing the dependency declaration.
    pub path: String,
    /// Dependency name whose requirement will be replaced.
    pub dependency: String,
    /// Existing Cargo version requirement.
    pub old: String,
    /// Baseline Cargo version requirement to write.
    pub new: String,
}
