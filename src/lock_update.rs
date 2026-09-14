// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Compatibility lock updates represented by synchronization plans.

/// A lockfile update requested after manifest synchronization.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::LockUpdate;
///
/// let update = LockUpdate {
///     package: "serde".into(),
///     version: "1.0".into(),
/// };
/// assert_eq!(update.version, "1.0");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockUpdate {
    /// Package name whose resolved requirement may need updating.
    pub package: String,
    /// Requested Cargo version requirement.
    pub version: String,
}
