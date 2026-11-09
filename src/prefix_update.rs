// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Prefix-based dependency source selection and manifest updates.

use std::collections::HashMap;
use std::process::Command;

use camino::Utf8Path;
use camino::Utf8PathBuf;
use semver::Version;
use toml_edit::DocumentMut;
use toml_edit::InlineTable;
use toml_edit::Item;
use toml_edit::TableLike;
use toml_edit::Value;
use toml_edit::value;

use crate::PolicyError;

#[derive(Debug, Clone)]
struct LocalCrate {
    path: Utf8PathBuf,
    version: Version,
}

#[derive(Debug, Clone)]
enum DependencySource {
    Registry,
    Path(Utf8PathBuf),
}

#[derive(Debug)]
struct Change {
    manifest: Utf8PathBuf,
    package: String,
    old: String,
    new: String,
}

#[derive(Debug)]
struct ManifestPlan {
    path: Utf8PathBuf,
    document: DocumentMut,
    changes: Vec<Change>,
}

/// Updates matching dependencies in every Cargo manifest below `project`.
///
/// Local crates found in ancestor directories take precedence only when their
/// stable version is newer than the latest stable crates.io release. Changes
/// are planned for all manifests before any file is written.
///
/// # Errors
///
/// Returns [`PolicyError::PrefixUpdate`] when a manifest is invalid, a
/// registry version cannot be queried, or a selected dependency cannot be
/// rewritten safely.
pub(crate) fn update_prefix_dependencies(
    project: &Utf8Path,
    prefixes: &[String],
    dry_run: bool,
) -> Result<(), PolicyError> {
    if prefixes.is_empty() || prefixes.iter().any(|prefix| prefix.is_empty()) {
        return Err(update_error(
            "at least one nonempty dependency prefix is required",
        ));
    }
    let project = project.canonicalize_utf8().map_err(|error| {
        update_error(format!(
            "failed to resolve project directory {project}: {error}"
        ))
    })?;
    let manifests = discover_manifests(&project)?;
    let mut latest_versions = HashMap::new();
    let mut local_crates = HashMap::new();
    let mut plans = Vec::new();

    for manifest in manifests {
        let source = std::fs::read_to_string(manifest.as_std_path()).map_err(|error| {
            update_error(format!("failed to read manifest {manifest}: {error}"))
        })?;
        let mut document = source.parse::<DocumentMut>().map_err(|error| {
            update_error(format!("failed to parse manifest {manifest}: {error}"))
        })?;
        let mut changes = Vec::new();
        let manifest_dir = manifest
            .parent()
            .ok_or_else(|| update_error(format!("manifest has no parent directory: {manifest}")))?;
        update_document(
            &mut document,
            &manifest,
            manifest_dir,
            prefixes,
            &mut latest_versions,
            &mut local_crates,
            &mut changes,
        )?;
        if !changes.is_empty() {
            plans.push(ManifestPlan {
                path: manifest,
                document,
                changes,
            });
        }
    }

    for plan in &plans {
        for change in &plan.changes {
            println!(
                "{}: {}: {} -> {}",
                change.manifest, change.package, change.old, change.new
            );
        }
    }
    if dry_run {
        return Ok(());
    }
    for plan in plans {
        std::fs::write(plan.path.as_std_path(), plan.document.to_string()).map_err(|error| {
            update_error(format!("failed to write manifest {}: {error}", plan.path))
        })?;
    }
    Ok(())
}

fn update_document(
    document: &mut DocumentMut,
    manifest: &Utf8Path,
    manifest_dir: &Utf8Path,
    prefixes: &[String],
    latest_versions: &mut HashMap<String, Option<Version>>,
    local_crates: &mut HashMap<(Utf8PathBuf, String), Option<LocalCrate>>,
    changes: &mut Vec<Change>,
) -> Result<(), PolicyError> {
    for table_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = document
            .get_mut(table_name)
            .and_then(Item::as_table_like_mut)
        {
            update_dependency_table(
                table,
                manifest,
                manifest_dir,
                prefixes,
                latest_versions,
                local_crates,
                changes,
            )?;
        }
    }
    if let Some(workspace) = document
        .get_mut("workspace")
        .and_then(Item::as_table_like_mut)
        && let Some(table) = workspace
            .get_mut("dependencies")
            .and_then(Item::as_table_like_mut)
    {
        update_dependency_table(
            table,
            manifest,
            manifest_dir,
            prefixes,
            latest_versions,
            local_crates,
            changes,
        )?;
    }
    if let Some(targets) = document.get_mut("target").and_then(Item::as_table_like_mut) {
        let target_names: Vec<_> = targets.iter().map(|(name, _)| name.to_owned()).collect();
        for target_name in target_names {
            let Some(target) = targets
                .get_mut(&target_name)
                .and_then(Item::as_table_like_mut)
            else {
                continue;
            };
            for table_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(table) = target.get_mut(table_name).and_then(Item::as_table_like_mut) {
                    update_dependency_table(
                        table,
                        manifest,
                        manifest_dir,
                        prefixes,
                        latest_versions,
                        local_crates,
                        changes,
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn update_dependency_table(
    table: &mut dyn TableLike,
    manifest: &Utf8Path,
    manifest_dir: &Utf8Path,
    prefixes: &[String],
    latest_versions: &mut HashMap<String, Option<Version>>,
    local_crates: &mut HashMap<(Utf8PathBuf, String), Option<LocalCrate>>,
    changes: &mut Vec<Change>,
) -> Result<(), PolicyError> {
    let names: Vec<_> = table.iter().map(|(name, _)| name.to_owned()).collect();
    for dependency_key in names {
        let Some(item) = table.get(&dependency_key) else {
            continue;
        };
        let package = dependency_package_name(item, &dependency_key);
        if !prefixes.iter().any(|prefix| package.starts_with(prefix))
            || uses_workspace_dependency(item)
        {
            continue;
        }
        let cache_key = (manifest_dir.to_owned(), package.clone());
        if !local_crates.contains_key(&cache_key) {
            local_crates.insert(cache_key.clone(), find_local_crate(manifest_dir, &package)?);
        }
        let local = local_crates.get(&cache_key).cloned().flatten();
        if !latest_versions.contains_key(&package) {
            latest_versions.insert(package.clone(), latest_stable_version(&package)?);
        }
        let latest = latest_versions.get(&package).cloned().flatten();
        let (source, version) = match (local, latest) {
            (Some(local), Some(latest)) if local.version > latest => (
                DependencySource::Path(relative_path(manifest_dir, &local.path)?),
                local.version,
            ),
            (Some(local), None) => (
                DependencySource::Path(relative_path(manifest_dir, &local.path)?),
                local.version,
            ),
            (_, Some(latest)) => (DependencySource::Registry, latest),
            (None, None) => {
                return Err(update_error(format!(
                    "crate {package} has no crates.io release and no matching local repository"
                )));
            }
        };
        let requirement = format!("^{}.{}", version.major, version.minor);
        let item = table
            .get_mut(&dependency_key)
            .expect("dependency key came from table");
        let old = item.to_string();
        update_dependency_item(item, &requirement, &source)?;
        let new = item.to_string();
        if old != new {
            let source_label = match source {
                DependencySource::Registry => "crates.io".to_owned(),
                DependencySource::Path(path) => format!("path {}", path),
            };
            changes.push(Change {
                manifest: manifest.to_owned(),
                package: package.clone(),
                old,
                new: format!("{new} ({source_label})"),
            });
        }
    }
    Ok(())
}

fn dependency_package_name(item: &Item, key: &str) -> String {
    dependency_table(item)
        .and_then(|table| table.get("package"))
        .and_then(Item::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| key.to_owned())
}

fn uses_workspace_dependency(item: &Item) -> bool {
    dependency_table(item)
        .and_then(|table| table.get("workspace"))
        .and_then(Item::as_bool)
        == Some(true)
}

fn dependency_table(item: &Item) -> Option<&dyn TableLike> {
    item.as_inline_table()
        .map(|table| table as &dyn TableLike)
        .or_else(|| item.as_table().map(|table| table as &dyn TableLike))
}

fn update_dependency_item(
    item: &mut Item,
    requirement: &str,
    source: &DependencySource,
) -> Result<(), PolicyError> {
    match item {
        Item::Value(Value::String(_)) => match source {
            DependencySource::Registry => *item = value(requirement),
            DependencySource::Path(path) => {
                let mut table = InlineTable::new();
                table.insert("version", Value::from(requirement));
                table.insert("path", Value::from(path.as_str()));
                *item = Item::Value(Value::InlineTable(table));
            }
        },
        Item::Value(Value::InlineTable(table)) => update_inline_table(table, requirement, source),
        Item::Table(table) => update_table(table, requirement, source),
        _ => {
            return Err(update_error(
                "dependency declaration is not a supported string or table",
            ));
        }
    }
    Ok(())
}

fn update_inline_table(table: &mut InlineTable, requirement: &str, source: &DependencySource) {
    table.insert("version", Value::from(requirement));
    match source {
        DependencySource::Registry => remove_source_keys(table),
        DependencySource::Path(path) => {
            remove_git_keys(table);
            table.insert("path", Value::from(path.as_str()));
        }
    }
}

fn update_table(table: &mut dyn TableLike, requirement: &str, source: &DependencySource) {
    table.insert("version", value(requirement));
    match source {
        DependencySource::Registry => remove_source_keys(table),
        DependencySource::Path(path) => {
            remove_git_keys(table);
            table.insert("path", value(path.as_str()));
        }
    }
}

fn remove_source_keys(table: &mut dyn TableLike) {
    for key in [
        "path",
        "git",
        "branch",
        "tag",
        "rev",
        "registry",
        "registry-index",
    ] {
        table.remove(key);
    }
}

fn remove_git_keys(table: &mut dyn TableLike) {
    for key in ["git", "branch", "tag", "rev", "registry", "registry-index"] {
        table.remove(key);
    }
}

fn discover_manifests(root: &Utf8Path) -> Result<Vec<Utf8PathBuf>, PolicyError> {
    let mut manifests = vec![root.join("Cargo.toml")];
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(directory.as_std_path()).map_err(|error| {
            update_error(format!("failed to read directory {directory}: {error}"))
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                update_error(format!("failed to read an entry in {directory}: {error}"))
            })?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "target" {
                continue;
            }
            let file_type = entry.file_type().map_err(|error| {
                update_error(format!(
                    "failed to inspect {}: {error}",
                    entry.path().display()
                ))
            })?;
            if !file_type.is_dir() {
                continue;
            }
            let path = Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| {
                update_error(format!("project path is not UTF-8: {}", path.display()))
            })?;
            if path.join("Cargo.toml").is_file() {
                manifests.push(path.join("Cargo.toml"));
            }
            pending.push(path);
        }
    }
    manifests.sort();
    manifests.dedup();
    Ok(manifests)
}

fn find_local_crate(
    manifest_dir: &Utf8Path,
    package: &str,
) -> Result<Option<LocalCrate>, PolicyError> {
    let mut newest: Option<LocalCrate> = None;
    let mut ancestor = Some(manifest_dir);
    while let Some(directory) = ancestor {
        let entries = std::fs::read_dir(directory.as_std_path()).map_err(|error| {
            update_error(format!(
                "failed to scan ancestor directory {directory}: {error}"
            ))
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                update_error(format!("failed to read an entry in {directory}: {error}"))
            })?;
            let file_type = entry.file_type().map_err(|error| {
                update_error(format!(
                    "failed to inspect {}: {error}",
                    entry.path().display()
                ))
            })?;
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') || name == "target" {
                continue;
            }
            let candidate_dir = Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| {
                update_error(format!("local crate path is not UTF-8: {}", path.display()))
            })?;
            let candidate_manifest = candidate_dir.join("Cargo.toml");
            if !candidate_manifest.is_file() {
                continue;
            }
            let Some((candidate_name, version)) = read_package(&candidate_manifest)? else {
                continue;
            };
            if candidate_name != package || !version.pre.is_empty() {
                continue;
            }
            let candidate = LocalCrate {
                path: candidate_dir,
                version,
            };
            if newest
                .as_ref()
                .is_none_or(|current| candidate.version > current.version)
            {
                newest = Some(candidate);
            }
        }
        ancestor = directory.parent();
    }
    Ok(newest)
}

fn read_package(manifest: &Utf8Path) -> Result<Option<(String, Version)>, PolicyError> {
    let Ok(text) = std::fs::read_to_string(manifest.as_std_path()) else {
        return Ok(None);
    };
    let Ok(document) = text.parse::<DocumentMut>() else {
        return Ok(None);
    };
    let Some(package) = document.get("package").and_then(Item::as_table_like) else {
        return Ok(None);
    };
    let Some(name) = package.get("name").and_then(Item::as_str) else {
        return Ok(None);
    };
    let version = package
        .get("version")
        .and_then(Item::as_str)
        .map(str::to_owned)
        .or_else(|| {
            let inherited = package
                .get("version")
                .and_then(Item::as_table_like)
                .and_then(|item| item.get("workspace"))
                .and_then(Item::as_bool)
                == Some(true);
            if inherited {
                manifest.parent().and_then(workspace_version)
            } else {
                None
            }
        });
    let Some(version) = version else {
        return Ok(None);
    };
    let Ok(version) = Version::parse(&version) else {
        return Ok(None);
    };
    Ok(Some((name.to_owned(), version)))
}

fn workspace_version(package_dir: &Utf8Path) -> Option<String> {
    let mut ancestor = Some(package_dir);
    while let Some(directory) = ancestor {
        let manifest = directory.join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(manifest.as_std_path())
            && let Ok(document) = text.parse::<DocumentMut>()
            && let Some(version) = document
                .get("workspace")
                .and_then(Item::as_table_like)
                .and_then(|workspace| workspace.get("package"))
                .and_then(Item::as_table_like)
                .and_then(|package| package.get("version"))
                .and_then(Item::as_str)
        {
            return Some(version.to_owned());
        }
        ancestor = directory.parent();
    }
    None
}

fn latest_stable_version(package: &str) -> Result<Option<Version>, PolicyError> {
    let url = format!("https://crates.io/api/v1/crates/{package}");
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--user-agent",
            "rs-infra-dependency prefix updater",
            "--write-out",
            "\n%{http_code}",
            &url,
        ])
        .output()
        .map_err(|error| update_error(format!("failed to start curl for {package}: {error}")))?;
    if !output.status.success() {
        return Err(update_error(format!(
            "crates.io request for {package} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let response = String::from_utf8(output.stdout).map_err(|error| {
        update_error(format!(
            "crates.io response for {package} is not UTF-8: {error}"
        ))
    })?;
    let (body, status) = response.rsplit_once('\n').ok_or_else(|| {
        update_error(format!(
            "crates.io response for {package} has no HTTP status"
        ))
    })?;
    if status == "404" {
        return Ok(None);
    }
    if status != "200" {
        return Err(update_error(format!(
            "crates.io returned HTTP {status} for {package}"
        )));
    }
    let json: serde_json::Value = serde_json::from_str(body).map_err(|error| {
        update_error(format!("invalid crates.io response for {package}: {error}"))
    })?;
    let version = json
        .pointer("/crate/max_stable_version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            update_error(format!(
                "crates.io response has no stable version for {package}"
            ))
        })?;
    let version = Version::parse(version).map_err(|error| {
        update_error(format!("invalid crates.io version for {package}: {error}"))
    })?;
    if !version.pre.is_empty() {
        return Err(update_error(format!(
            "crates.io latest version for {package} is not stable"
        )));
    }
    Ok(Some(version))
}

fn relative_path(from: &Utf8Path, to: &Utf8Path) -> Result<Utf8PathBuf, PolicyError> {
    let from = from.canonicalize_utf8().map_err(|error| {
        update_error(format!(
            "failed to resolve manifest directory {from}: {error}"
        ))
    })?;
    let to = to.canonicalize_utf8().map_err(|error| {
        update_error(format!("failed to resolve local crate path {to}: {error}"))
    })?;
    let common = from
        .ancestors()
        .find(|ancestor| to.starts_with(ancestor))
        .ok_or_else(|| update_error(format!("cannot compute relative path from {from} to {to}")))?;
    let from_tail = from
        .strip_prefix(common)
        .map_err(|error| update_error(error.to_string()))?;
    let to_tail = to
        .strip_prefix(common)
        .map_err(|error| update_error(error.to_string()))?;
    let mut relative = Utf8PathBuf::new();
    for _ in from_tail.components() {
        relative.push("..");
    }
    relative.push(to_tail);
    if relative.as_str().is_empty() {
        relative.push(".");
    }
    Ok(relative)
}

fn update_error(message: impl Into<String>) -> PolicyError {
    PolicyError::PrefixUpdate {
        message: message.into(),
    }
}
