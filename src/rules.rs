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
use crate::cargo::load_locked_metadata;
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
/// * `config` - Configuration controlling first-party dependency
///   classification.
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
    let metadata = if baseline.baseline.has_resolved_rules() {
        load_locked_metadata(project)?
    } else {
        metadata
    };
    if baseline.baseline.has_resolved_rules() {
        check_resolved_dependencies(&metadata, &baseline.baseline, &mut violations);
    }
    Ok(Evaluation {
        violations,
        packages: resolved_packages(&metadata),
    })
}

fn check_resolved_dependencies(
    metadata: &cargo_metadata::Metadata,
    baseline: &Baseline,
    violations: &mut Vec<Violation>,
) {
    for package in &metadata.packages {
        let Some(expected) = baseline
            .resolved_iter()
            .find_map(|(name, requirement)| (name == package.name.as_str()).then_some(requirement))
        else {
            continue;
        };
        if !expected.version().matches(&package.version) {
            let source = package
                .source
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "path".into());
            violations.push(Violation {
                code: "DP401",
                crate_name: package.name.to_string(),
                message: format!(
                    "resolved version {} from {source} is below minimum {}",
                    package.version,
                    expected.text()
                ),
                exception_id: None,
            });
        }
    }
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

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use cargo_metadata::MetadataCommand;

    use super::check_resolved_dependencies;
    use crate::Baseline;
    use crate::Violation;

    #[test]
    fn reports_resolved_packages_below_the_baseline_minimum() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let project = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 project path");
        std::fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = \"rules-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("manifest");
        std::fs::create_dir_all(project.join("src")).expect("source directory");
        std::fs::write(project.join("src/lib.rs"), "").expect("source");

        let metadata = MetadataCommand::new()
            .manifest_path(project.join("Cargo.toml").as_std_path())
            .no_deps()
            .exec()
            .expect("Cargo metadata");
        let baseline = Baseline::parse_toml(
            "format = 3\n[resolved]\nrules-fixture = \">=1.0.0\"\nmissing-package = \">=1.0.0\"\n",
        )
        .expect("resolved baseline");
        let mut violations: Vec<Violation> = Vec::new();

        check_resolved_dependencies(&metadata, &baseline, &mut violations);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].code, "DP401");
        assert_eq!(violations[0].crate_name, "rules-fixture");
        assert!(violations[0].message.contains("from path"));
    }
}
