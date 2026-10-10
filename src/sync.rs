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

/// Plans safe direct-dependency version replacements in standard dependency
/// tables.
///
/// The plan covers registry-style string and inline-table declarations while
/// leaving path and workspace dependencies unchanged. Inline declarations that
/// cannot be safely rewritten are recorded in `SyncPlan::blocked`.
///
/// # Errors
///
/// Returns [`PolicyError::Sync`] when the manifest cannot be read or parsed.
///
/// # Parameters
///
/// * `project` - Project root containing the Cargo manifest.
/// * `baseline` - Policy requirements used to construct edits.
///
/// # Returns
///
/// Returns safe manifest edits, compatibility lock updates, and blocked items.
pub fn plan_sync(project: &Utf8Path, baseline: &Baseline) -> Result<SyncPlan, PolicyError> {
    let manifest_path = project.join("Cargo.toml");
    let source = std::fs::read_to_string(manifest_path.as_std_path()).map_err(|error| PolicyError::Sync {
        message: format!("failed to read {manifest_path}: {error}"),
    })?;
    let document = source.parse::<DocumentMut>().map_err(|error| PolicyError::Sync {
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
                    Some(old) if old != requirement.text() => {
                        add_edit(&mut plan, &manifest_path, name, old.into(), requirement.text())
                    }
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
///
/// Each edit is re-read and applied to the current manifest. The operation
/// stops before writing when the plan contains blocked entries or when an edit
/// no longer matches a supported dependency declaration.
///
/// # Errors
///
/// Returns [`PolicyError::Sync`] when the plan is blocked, a manifest cannot be
/// read or parsed, an edit target disappeared, or a write fails.
///
/// # Parameters
///
/// * `plan` - Synchronization plan whose safe edits should be applied.
///
/// # Returns
///
/// Returns `Ok(())` after all manifest edits are written successfully.
pub fn apply_sync(plan: &SyncPlan) -> Result<(), PolicyError> {
    if !plan.blocked.is_empty() {
        return Err(PolicyError::Sync {
            message: "sync plan contains blocked edits".into(),
        });
    }
    for edit in &plan.manifest_edits {
        let path = Utf8Path::new(&edit.path);
        let source = std::fs::read_to_string(path.as_std_path()).map_err(|error| PolicyError::Sync {
            message: format!("failed to read {}: {error}", edit.path),
        })?;
        let mut document = source.parse::<DocumentMut>().map_err(|error| PolicyError::Sync {
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
        std::fs::write(path.as_std_path(), document.to_string()).map_err(|error| PolicyError::Sync {
            message: format!("failed to write {}: {error}", edit.path),
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::apply_sync;
    use super::plan_sync;
    use crate::Baseline;
    use crate::FileEdit;
    use crate::SyncPlan;

    #[test]
    fn applies_sync_edits_and_reports_read_and_parse_failures() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let project = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 project path");
        let manifest = project.join("Cargo.toml");
        std::fs::write(&manifest, "[dependencies]\nserde = \"1.0\"\n").expect("manifest");
        let baseline = Baseline::parse("serde 2.0\n").expect("baseline");
        let plan = plan_sync(&project, &baseline).expect("sync plan");

        apply_sync(&plan).expect("apply manifest edit");
        assert!(
            std::fs::read_to_string(&manifest)
                .expect("updated manifest")
                .contains("serde = \"2.0\"")
        );

        std::fs::write(&manifest, "not = [valid toml").expect("invalid manifest");
        let parse_failure = SyncPlan {
            manifest_edits: vec![FileEdit {
                path: manifest.to_string(),
                dependency: "serde".into(),
                old: "1.0".into(),
                new: "2.0".into(),
            }],
            lock_updates: Vec::new(),
            blocked: Vec::new(),
        };
        assert!(apply_sync(&parse_failure).is_err());

        let read_failure = SyncPlan {
            manifest_edits: vec![FileEdit {
                path: project.join("missing.toml").to_string(),
                dependency: "serde".into(),
                old: "1.0".into(),
                new: "2.0".into(),
            }],
            lock_updates: Vec::new(),
            blocked: Vec::new(),
        };
        assert!(apply_sync(&read_failure).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn reports_a_manifest_write_failure() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let project = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 project path");
        let manifest = project.join("Cargo.toml");
        std::fs::write(&manifest, "[dependencies]\nserde = \"1.0\"\n").expect("manifest");
        let plan = plan_sync(&project, &Baseline::parse("serde 2.0\n").expect("baseline")).expect("sync plan");
        let mut permissions = std::fs::metadata(&manifest).expect("manifest metadata").permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&manifest, permissions).expect("make manifest read-only");

        let error = apply_sync(&plan).expect_err("read-only manifest cannot be updated");

        assert!(error.to_string().contains("failed to write"));
    }
}
