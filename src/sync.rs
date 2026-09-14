// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Direct dependency-version synchronization planning and application.

use camino::Utf8Path;
use toml_edit::DocumentMut;
use toml_edit::Item;
use toml_edit::Value;
use toml_edit::value;

use crate::Baseline;
use crate::PolicyError;
use crate::Violation;

#[path = "file_edit.rs"]
mod file_edit;
#[path = "lock_update.rs"]
mod lock_update;
#[path = "sync_plan.rs"]
mod sync_plan;

pub use file_edit::FileEdit;
pub use lock_update::LockUpdate;
pub use sync_plan::SyncPlan;

/// Plans safe direct-dependency version replacements in standard dependency tables.
pub fn plan_sync(project: &Utf8Path, baseline: &Baseline) -> Result<SyncPlan, PolicyError> {
    let manifest_path = project.join("Cargo.toml");
    let source = std::fs::read_to_string(manifest_path.as_std_path()).map_err(|error| {
        PolicyError::Sync {
            message: format!("failed to read {manifest_path}: {error}"),
        }
    })?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|error| PolicyError::Sync {
            message: format!("failed to parse {manifest_path}: {error}"),
        })?;
    let mut plan = SyncPlan {
        manifest_edits: Vec::new(),
        lock_updates: Vec::new(),
        blocked: Vec::new(),
    };
    for table_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(table) = document.get(table_name).and_then(Item::as_table_like) else {
            continue;
        };
        for (name, item) in table.iter() {
            let Some(requirement) = baseline.requirement(name) else {
                continue;
            };
            if let Item::Value(Value::String(old)) = item {
                let old = old.value().to_owned();
                if old != requirement.text() {
                    add_edit(&mut plan, &manifest_path, name, old, requirement.text());
                }
            } else if let Some(inline) = item.as_inline_table() {
                if inline.get("path").is_some() || inline.get("workspace").is_some() {
                    continue;
                }
                match inline.get("version").and_then(Value::as_str) {
                    Some(old) if old != requirement.text() => add_edit(
                        &mut plan,
                        &manifest_path,
                        name,
                        old.into(),
                        requirement.text(),
                    ),
                    Some(_) => {}
                    None => plan.blocked.push(Violation {
                        code: "DP301",
                        crate_name: name.into(),
                        message: "external dependency has no version field".into(),
                        exception_id: None,
                    }),
                }
            }
        }
    }
    Ok(plan)
}

/// Adds one manifest replacement and its corresponding compatibility update.
fn add_edit(plan: &mut SyncPlan, path: &Utf8Path, name: &str, old: String, new: &str) {
    plan.manifest_edits.push(FileEdit {
        path: path.to_string(),
        dependency: name.into(),
        old,
        new: new.into(),
    });
    plan.lock_updates.push(LockUpdate {
        package: name.into(),
        version: new.into(),
    });
}

/// Applies manifest edits from a synchronization plan.
pub fn apply_sync(plan: &SyncPlan) -> Result<(), PolicyError> {
    if !plan.blocked.is_empty() {
        return Err(PolicyError::Sync {
            message: "sync plan contains blocked edits".into(),
        });
    }
    for edit in &plan.manifest_edits {
        let path = Utf8Path::new(&edit.path);
        let source =
            std::fs::read_to_string(path.as_std_path()).map_err(|error| PolicyError::Sync {
                message: format!("failed to read {}: {error}", edit.path),
            })?;
        let mut document = source
            .parse::<DocumentMut>()
            .map_err(|error| PolicyError::Sync {
                message: format!("failed to parse {}: {error}", edit.path),
            })?;
        let mut updated = false;
        for table_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
            let Some(item) = document
                .get_mut(table_name)
                .and_then(|table| table.get_mut(&edit.dependency))
            else {
                continue;
            };
            if let Item::Value(Value::String(_)) = item {
                *item = value(edit.new.clone());
                updated = true;
                break;
            }
            if let Some(inline) = item.as_inline_table_mut() {
                inline.insert("version", Value::from(edit.new.clone()));
                updated = true;
                break;
            }
        }
        if !updated {
            return Err(PolicyError::Sync {
                message: format!("dependency {} disappeared", edit.dependency),
            });
        }
        std::fs::write(path.as_std_path(), document.to_string()).map_err(|error| {
            PolicyError::Sync {
                message: format!("failed to write {}: {error}", edit.path),
            }
        })?;
    }
    Ok(())
}
