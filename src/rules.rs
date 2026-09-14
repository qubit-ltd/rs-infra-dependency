// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Direct third-party dependency baseline evaluation.

use camino::Utf8Path;

use crate::Baseline;
use crate::LoadedBaseline;
use crate::PolicyError;
use crate::ProjectConfig;
use crate::cargo::load_metadata;
use crate::cargo::resolved_packages;
use crate::diagnostic::Violation;

#[path = "evaluation.rs"]
mod evaluation;

pub use evaluation::Evaluation;

/// Evaluates external direct dependencies against the selected baseline.
///
/// Internal, path, and workspace dependencies are excluded according to
/// `config`. The returned package list is the Cargo metadata projection used by
/// reports, while violations describe only direct external declarations.
///
/// # Errors
///
/// Returns [`PolicyError::Cargo`] when Cargo metadata cannot be loaded.
///
/// # Parameters
///
/// * `project` - Cargo project root whose direct dependencies are evaluated.
/// * `config` - Configuration controlling first-party dependency classification.
/// * `baseline` - Loaded policy baseline used for requirement comparisons.
///
/// # Returns
///
/// Returns the violations and resolved packages observed for the project.
pub fn evaluate(
    project: &Utf8Path,
    config: &ProjectConfig,
    baseline: &LoadedBaseline,
) -> Result<Evaluation, PolicyError> {
    let (metadata, root) = load_metadata(project)?;
    let mut violations = Vec::new();
    if let Some(root) = root {
        check_direct_dependencies(&root, config, &baseline.baseline, &mut violations);
    }
    Ok(Evaluation {
        violations,
        packages: resolved_packages(&metadata),
    })
}

fn check_direct_dependencies(
    root: &cargo_metadata::Package,
    config: &ProjectConfig,
    baseline: &Baseline,
    violations: &mut Vec<Violation>,
) {
    for dependency in &root.dependencies {
        if config.is_internal_dependency(dependency) {
            continue;
        }
        let Some(expected) = baseline.requirement(&dependency.name) else {
            violations.push(Violation {
                code: "DP203",
                crate_name: dependency.name.clone(),
                message: "external direct dependency is absent from the baseline".into(),
                exception_id: None,
            });
            continue;
        };
        if dependency.req != *expected.version() {
            violations.push(Violation {
                code: "DP202",
                crate_name: dependency.name.clone(),
                message: format!(
                    "declared requirement {} differs from baseline {}",
                    dependency.req,
                    expected.text()
                ),
                exception_id: None,
            });
        }
    }
}
