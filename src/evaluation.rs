// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Evaluation results returned by dependency policy checks.

use crate::ResolvedPackage;
use crate::Violation;

/// Results of evaluating one project.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::Evaluation;
///
/// let evaluation = Evaluation {
///     violations: Vec::new(),
///     packages: Vec::new(),
/// };
/// assert!(evaluation.violations.is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct Evaluation {
    /// Violations found by the evaluator, in dependency traversal order.
    pub violations: Vec<Violation>,
    /// Packages in the resolved dependency graph, included for reports only.
    pub packages: Vec<ResolvedPackage>,
}
