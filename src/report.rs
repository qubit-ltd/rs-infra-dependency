// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Policy report data models and renderers.

use serde::Serialize;

use crate::Evaluation;
use crate::LoadedBaseline;
use crate::PolicyError;

// qubit-style: allow multiple-public-types

/// Stable identity of the baseline used for a report.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::BaselineIdentity;
///
/// let identity = BaselineIdentity {
///     release: "v2026.09.0".into(),
///     revision: "0123456789abcdef0123456789abcdef01234567".into(),
///     name: "v2026.09.0".into(),
/// };
/// assert_eq!(identity.release, "v2026.09.0");
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct BaselineIdentity {
    /// Baseline release name.
    pub release: String,
    /// Commit SHA selected by the project.
    pub revision: String,
    /// Human-readable baseline name.
    pub name: String,
}

/// Machine-readable and human-readable policy report data.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::{BaselineIdentity, Report};
///
/// let report = Report {
///     schema_version: 1,
///     baseline: BaselineIdentity {
///         release: "v2026.09.13".into(),
///         revision: "0123456789abcdef0123456789abcdef01234567".into(),
///         name: "example".into(),
///     },
///     violations: Vec::new(),
///     packages: Vec::new(),
/// };
/// assert!(report.violations.is_empty());
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// Report schema version.
    pub schema_version: u32,
    /// Baseline identity.
    pub baseline: BaselineIdentity,
    /// Found policy violations.
    pub violations: Vec<crate::Violation>,
    /// Resolved package graph.
    pub packages: Vec<crate::ResolvedPackage>,
}

impl Report {
    /// Builds a report from an evaluation and its selected baseline.
    ///
    /// The evaluation is consumed because its violations and resolved package
    /// records become owned report fields. The baseline identity is copied into
    /// the report so the result remains self-contained.
    ///
    /// # Parameters
    ///
    /// * `evaluation` - Evaluation result to move into the report.
    /// * `baseline` - Loaded baseline whose identity is copied.
    ///
    /// # Returns
    ///
    /// Returns a report with schema version `1` and the supplied data.
    pub fn from_evaluation(evaluation: Evaluation, baseline: &LoadedBaseline) -> Self {
        Self {
            schema_version: 1,
            baseline: BaselineIdentity {
                release: baseline.release.clone(),
                revision: baseline.commit.clone(),
                name: baseline.release.clone(),
            },
            violations: evaluation.violations,
            packages: evaluation.packages,
        }
    }
}

/// Serializes a report as stable, pretty-printed JSON.
///
/// # Errors
///
/// Returns [`PolicyError::Report`] if serialization fails.
///
/// # Parameters
///
/// * `report` - Report data to serialize.
///
/// # Returns
///
/// Returns pretty-printed JSON text on success.
pub fn render_json(report: &Report) -> Result<String, PolicyError> {
    serde_json::to_string_pretty(report).map_err(|error| PolicyError::Report {
        message: error.to_string(),
    })
}

/// Serializes a report as concise Markdown, including the selected baseline and
/// every policy violation.
///
/// # Parameters
///
/// * `report` - Report data to render.
///
/// # Returns
///
/// Returns the rendered Markdown document.
pub fn render_markdown(report: &Report) -> String {
    let mut output = format!(
        "# Dependency Policy Report\n\n- Baseline: ~{}~\n- Revision: ~{}~\n\n",
        report.baseline.name, report.baseline.revision
    );
    if report.violations.is_empty() {
        output.push_str("No violations found.\n");
    } else {
        output.push_str("## Violations\n\n");
        for violation in &report.violations {
            output.push_str(&format!(
                "- {} {}: {}\n",
                violation.code, violation.crate_name, violation.message
            ));
        }
    }
    output
}
