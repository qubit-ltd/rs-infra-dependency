// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use camino::Utf8Path;
use camino::Utf8PathBuf;
use qubit_infra_dependency::Baseline;
use qubit_infra_dependency::FileEdit;
use qubit_infra_dependency::LockUpdate;
use qubit_infra_dependency::SyncPlan;
use qubit_infra_dependency::Violation;
use qubit_infra_dependency::apply_sync;
use qubit_infra_dependency::plan_sync;

fn baseline() -> Baseline {
    Baseline::parse("num-bigint 0.4\n").expect("valid text baseline")
}

#[test]
fn plans_num_bigint_version_replacement_without_writing() {
    let plan = plan_sync(Utf8Path::new("tests/fixtures/sync-version-only"), &baseline()).expect("sync plan");
    assert_eq!(plan.manifest_edits.len(), 1);
    assert_eq!(plan.manifest_edits[0].dependency, "num-bigint");
    assert_eq!(plan.manifest_edits[0].new, "0.4");
}

#[test]
fn blocks_inline_dependency_declarations() {
    let plan = plan_sync(Utf8Path::new("tests/fixtures/sync-feature-conflict"), &baseline()).expect("sync plan");
    assert_eq!(plan.manifest_edits[0].new, "0.4");
}

#[test]
fn plans_and_applies_string_and_inline_dependency_replacements() {
    let directory = tempfile::tempdir().expect("temporary project");
    let project = Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).expect("UTF-8 path");
    std::fs::write(
        project.join("Cargo.toml"),
        "[dependencies]\nserde = \"1.0\"\nregex = { version = \"1.0\", features = [\"std\"] }\n[dev-dependencies]\nserde_json = \"1.0\"\n",
    )
    .expect("manifest");
    let baseline = Baseline::parse("serde 2.0\nregex 2.0\nserde_json 2.0\n").expect("baseline");
    let plan = plan_sync(&project, &baseline).expect("plan");
    assert_eq!(plan.manifest_edits.len(), 3);
    assert_eq!(plan.lock_updates.len(), 3);
    apply_sync(&plan).expect("apply");
    let manifest = std::fs::read_to_string(project.join("Cargo.toml")).expect("updated manifest");
    assert!(manifest.contains("serde = \"2.0\""));
    assert!(manifest.contains("regex = { version = \"2.0\""));
    assert!(manifest.contains("serde_json = \"2.0\""));
}

#[test]
fn reports_plan_and_manifest_errors() {
    let directory = tempfile::tempdir().expect("temporary project");
    let project = Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).expect("UTF-8 path");
    assert!(plan_sync(&project, &baseline()).is_err());

    std::fs::write(project.join("Cargo.toml"), "not = [valid toml").expect("invalid manifest");
    assert!(plan_sync(&project, &baseline()).is_err());

    let blocked = SyncPlan {
        manifest_edits: vec![],
        lock_updates: vec![],
        blocked: vec![Violation {
            code: "DP301",
            crate_name: "serde".into(),
            message: "manual update required".into(),
            exception_id: None,
        }],
    };
    assert!(apply_sync(&blocked).is_err());
}

#[test]
fn rejects_edits_for_disappeared_dependencies_and_invalid_manifests() {
    let directory = tempfile::tempdir().expect("temporary project");
    let project = Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).expect("UTF-8 path");
    let manifest = project.join("Cargo.toml");
    std::fs::write(&manifest, "[dependencies]\nserde = \"1.0\"\n").expect("manifest");
    let edit = |dependency: &str| FileEdit {
        path: manifest.to_string(),
        dependency: dependency.into(),
        old: "1.0".into(),
        new: "2.0".into(),
    };
    assert!(
        apply_sync(&SyncPlan {
            manifest_edits: vec![edit("missing")],
            lock_updates: vec![],
            blocked: vec![]
        })
        .is_err()
    );
    std::fs::write(&manifest, "not = [valid toml").expect("invalid manifest");
    assert!(
        apply_sync(&SyncPlan {
            manifest_edits: vec![edit("serde")],
            lock_updates: vec![],
            blocked: vec![]
        })
        .is_err()
    );
    let missing_edit = FileEdit {
        path: project.join("missing.toml").to_string(),
        dependency: "serde".into(),
        old: "1.0".into(),
        new: "2.0".into(),
    };
    assert!(
        apply_sync(&SyncPlan {
            manifest_edits: vec![missing_edit],
            lock_updates: vec![],
            blocked: vec![],
        })
        .is_err()
    );
    assert_eq!(
        LockUpdate {
            package: "serde".into(),
            version: "2.0".into()
        }
        .version,
        "2.0"
    );
}

#[test]
fn skips_unchanged_local_and_workspace_dependencies_and_blocks_missing_versions() {
    let directory = tempfile::tempdir().expect("temporary project");
    let project = Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).expect("UTF-8 path");
    std::fs::write(
        project.join("Cargo.toml"),
        "[dependencies]\nsame = \"1.0\"\nunchanged = { version = \"1.0\" }\nlocal = { path = \"../local\" }\ninherited = { workspace = true }\nmissing = { features = [] }\nexternal = { version = \"1.0\" }\nunlisted = \"1.0\"\n",
    )
    .expect("manifest");
    let baseline = Baseline::parse("same 1.0\nunchanged 1.0\nlocal 2.0\ninherited 2.0\nmissing 2.0\nexternal 2.0\n")
        .expect("baseline");
    let plan = plan_sync(&project, &baseline).expect("sync plan");
    assert_eq!(plan.manifest_edits.len(), 1);
    assert_eq!(plan.manifest_edits[0].dependency, "external");
    assert_eq!(plan.blocked.len(), 1);
    assert_eq!(plan.blocked[0].code, "DP301");
}
