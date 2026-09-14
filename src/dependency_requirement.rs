// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! A parsed Cargo requirement retained by a dependency baseline.

use semver::VersionReq;

/// One declared Cargo version requirement from a baseline line.
///
/// Instances are obtained from [`crate::Baseline::requirement`] so the
/// original text and parsed requirement stay consistent.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::Baseline;
///
/// let baseline = Baseline::parse("serde 1.0\n").expect("valid baseline");
/// let requirement = baseline.requirement("serde").expect("serde rule");
/// assert_eq!(requirement.text(), "1.0");
/// ```
#[derive(Debug, Clone)]
pub struct DependencyRequirement {
    /// Original requirement text as it appeared in the baseline.
    ///
    /// This is retained so reports and synchronization plans can reproduce the
    /// policy's chosen spelling rather than formatting the parsed requirement.
    pub(crate) text: String,
    /// Parsed semantic-version requirement used for comparisons.
    pub(crate) version: VersionReq,
}

impl DependencyRequirement {
    /// Returns the original Cargo requirement text from the baseline.
    ///
    /// The returned string is borrowed from this requirement and is suitable
    /// for displaying or writing the policy's exact spelling.
    ///
    /// # Returns
    ///
    /// Returns the baseline text without allocating a new string.
    #[must_use]
    #[inline]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the parsed Cargo requirement used for comparisons.
    ///
    /// The returned value borrows the parsed requirement and performs no
    /// allocation.
    ///
    /// # Returns
    ///
    /// Returns the parsed semantic-version requirement.
    #[must_use]
    #[inline]
    pub fn version(&self) -> &VersionReq {
        &self.version
    }
}
