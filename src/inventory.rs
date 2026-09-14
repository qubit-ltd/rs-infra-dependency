// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Dependency inventory scanning and report rendering.

use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use cargo_metadata::DependencyKind;
use cargo_metadata::MetadataCommand;
use serde::Serialize;

use crate::PolicyError;

// qubit-style: allow multiple-public-types

/// A dependency declared by a package in an inventory scan.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InventoryDependency {
    /// Package declaring the dependency.
    pub package: String,
    /// Dependency name as it appears in Cargo.toml.
    pub name: String,
    /// Declared Cargo version requirement, if this is a registry dependency.
    pub requirement: Option<String>,
    /// Dependency origin (`registry`, `git`, `path`, or `workspace`).
    pub source: String,
    /// Dependency kind (`normal`, `build`, or `dev`).
    pub kind: String,
    /// Whether the dependency is optional.
    pub optional: bool,
}

/// A package resolved while scanning one project root.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InventoryProject {
    /// Project root.
    pub project: Utf8PathBuf,
    /// Packages belonging to this project/workspace.
    pub packages: Vec<String>,
    /// Direct dependency declarations found in the workspace.
    pub dependencies: Vec<InventoryDependency>,
    /// Resolved packages in Cargo's dependency graph.
    pub resolved: Vec<crate::ResolvedPackage>,
}

/// Complete result of a multi-project inventory scan.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::Inventory;
///
/// let inventory = Inventory {
///     schema_version: 1,
///     projects: Vec::new(),
///     direct_requirements: Default::default(),
///     conflicts: Default::default(),
/// };
/// assert!(inventory.projects.is_empty());
/// ```
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Inventory {
    /// Inventory schema version.
    pub schema_version: u32,
    /// Scanned projects.
    pub projects: Vec<InventoryProject>,
    /// Requirements observed for each direct dependency.
    pub direct_requirements: BTreeMap<String, Vec<String>>,
    /// Names whose declarations disagree and therefore need a policy decision.
    pub conflicts: BTreeMap<String, Vec<String>>,
}

/// Scan project roots without changing manifests, lockfiles, or the baseline.
pub fn scan_projects(projects: &[Utf8PathBuf]) -> Result<Inventory, PolicyError> {
    let mut scanned = Vec::with_capacity(projects.len());
    for project in projects {
        let manifest = project.join("Cargo.toml");
        let mut command = MetadataCommand::new();
        command.manifest_path(manifest.as_std_path());
        let metadata = command.exec().map_err(|error| PolicyError::Cargo {
            code: "DP201",
            message: format!("{}: {}", project, error),
        })?;
        let members = metadata.workspace_packages();
        let packages = members
            .iter()
            .map(|p| p.name.to_string())
            .collect::<Vec<_>>();
        let dependencies = members
            .iter()
            .flat_map(|package| {
                package
                    .dependencies
                    .iter()
                    .map(move |dependency| InventoryDependency {
                        package: package.name.to_string(),
                        name: dependency.name.to_string(),
                        requirement: Some(dependency.req.to_string()),
                        source: if dependency.path.is_some() {
                            "path"
                        } else if dependency.source.is_some() {
                            "registry-or-git"
                        } else {
                            "workspace"
                        }
                        .into(),
                        kind: match dependency.kind {
                            DependencyKind::Normal => "normal",
                            DependencyKind::Build => "build",
                            DependencyKind::Development => "dev",
                            _ => "other",
                        }
                        .into(),
                        optional: dependency.optional,
                    })
            })
            .collect();
        let resolved = metadata
            .packages
            .iter()
            .map(|package| crate::ResolvedPackage {
                name: package.name.to_string(),
                version: package.version.clone(),
                source: package.source.as_ref().map(ToString::to_string),
            })
            .collect();
        scanned.push(InventoryProject {
            project: project.clone(),
            packages,
            dependencies,
            resolved,
        });
    }

    let mut observed: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for project in &scanned {
        for dependency in &project.dependencies {
            if dependency.source != "workspace"
                && dependency.source != "path"
                && let Some(requirement) = &dependency.requirement
                && !observed
                    .entry(dependency.name.clone())
                    .or_default()
                    .contains(requirement)
            {
                observed
                    .get_mut(&dependency.name)
                    .unwrap()
                    .push(requirement.clone());
            }
        }
    }
    let conflicts = observed
        .iter()
        .filter(|(_, values)| values.len() > 1)
        .map(|(name, values)| (name.clone(), values.clone()))
        .collect();
    Ok(Inventory {
        schema_version: 1,
        projects: scanned,
        direct_requirements: observed,
        conflicts,
    })
}

/// Render an inventory as stable JSON.
pub fn render_inventory_json(inventory: &Inventory) -> Result<String, PolicyError> {
    serde_json::to_string_pretty(inventory).map_err(|error| PolicyError::Report {
        message: error.to_string(),
    })
}

/// Render an inventory as a concise Markdown review artifact.
pub fn render_inventory_markdown(inventory: &Inventory) -> String {
    let mut output = format!(
        "# Dependency Inventory\n\nProjects scanned: {}\n\n",
        inventory.projects.len()
    );
    output.push_str("| Dependency | Declared requirements |\n|---|---|\n");
    for (name, requirements) in &inventory.direct_requirements {
        output.push_str(&format!("| `{name}` | `{}` |\n", requirements.join("`, `")));
    }
    if !inventory.conflicts.is_empty() {
        output.push_str("\n## Conflicts requiring baseline decisions\n\n");
        for (name, requirements) in &inventory.conflicts {
            output.push_str(&format!("- `{name}`: {}\n", requirements.join(", ")));
        }
    }
    output
}
