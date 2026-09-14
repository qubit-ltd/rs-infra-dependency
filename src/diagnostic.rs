// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Stable diagnostics emitted by policy evaluation.

use serde::Serialize;

// qubit-style: allow type-file-name

/// A policy violation with a stable machine-readable code.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::Violation;
///
/// let violation = Violation {
///     code: "DP203",
///     crate_name: "serde".into(),
///     message: "dependency is absent from the baseline".into(),
///     exception_id: None,
/// };
/// assert_eq!(violation.code, "DP203");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Violation {
    /// Stable diagnostic code.
    pub code: &'static str,
    /// Package associated with the violation.
    pub crate_name: String,
    /// Human-readable explanation.
    pub message: String,
    /// Approved exception suppressing this violation, when present.
    pub exception_id: Option<String>,
}
