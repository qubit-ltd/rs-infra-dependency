// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use camino::Utf8PathBuf;
use qubit_infra_dependency::render_inventory_json;
use qubit_infra_dependency::render_inventory_markdown;
use qubit_infra_dependency::scan_projects;
use tempfile::tempdir;

#[test]
fn scans_direct_requirements_and_resolved_graph() {
    let inventory = scan_projects(&[Utf8PathBuf::from("tests/fixtures/num-bigint-05")]).expect("inventory");
    assert_eq!(inventory.projects.len(), 1);
    assert!(inventory.direct_requirements.contains_key("num-bigint"));
    assert!(inventory.projects[0].resolved.iter().any(|p| p.name == "num-bigint"));
    assert!(render_inventory_markdown(&inventory).contains("num-bigint"));
    assert!(
        render_inventory_json(&inventory)
            .expect("JSON inventory")
            .contains("num-bigint")
    );
}

#[test]
fn reports_requirement_conflicts_across_projects() {
    let inventory = scan_projects(&[
        Utf8PathBuf::from("tests/fixtures/num-bigint-05"),
        Utf8PathBuf::from("tests/fixtures/num-bigint-04"),
    ])
    .expect("inventory");
    assert!(inventory.conflicts.contains_key("num-bigint"));
    assert!(render_inventory_markdown(&inventory).contains("Conflicts requiring baseline decisions"));
}

#[test]
fn renders_an_empty_inventory_and_rejects_invalid_roots() {
    let empty = scan_projects(&[]).expect("empty inventory");
    assert!(empty.projects.is_empty());
    assert!(render_inventory_markdown(&empty).contains("Projects scanned: 0"));
    assert!(scan_projects(&[Utf8PathBuf::from("tests/fixtures/missing-inventory")]).is_err());
}

#[test]
fn reports_cargo_metadata_errors_for_invalid_project_manifests() {
    let temporary = tempdir().expect("temporary project");
    let project = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 project");
    std::fs::create_dir_all(project.join(".infra/tools")).expect("tool defaults directory");
    std::fs::write(
        project.join(".infra/tools/defaults.toml"),
        "build_toolchain = \"1.94.0\"\n",
    )
    .expect("tool defaults");
    std::fs::write(project.join("Cargo.toml"), "not = [valid toml").expect("invalid manifest");

    let error = scan_projects(&[project]).expect_err("invalid manifest should fail metadata loading");

    assert!(error.to_string().contains("DP201"));
}

#[test]
fn inventories_path_dependencies_by_kind_and_optional_status() {
    let temporary = tempdir().expect("temporary workspace");
    let project = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 project");
    std::fs::create_dir_all(project.join("src")).expect("project source directory");
    std::fs::write(project.join("src/lib.rs"), "").expect("project source");
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"inventory-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\nnormal-local = { path = \"normal-local\", optional = true }\n[build-dependencies]\nbuild-local = { path = \"build-local\" }\n[dev-dependencies]\ndev-local = { path = \"dev-local\" }\n",
    )
    .expect("workspace manifest");
    for name in ["normal-local", "build-local", "dev-local"] {
        std::fs::create_dir_all(project.join(name).join("src")).expect("path dependency source directory");
        std::fs::write(
            project.join(name).join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        )
        .expect("path dependency manifest");
        std::fs::write(project.join(name).join("src/lib.rs"), "").expect("path dependency source");
    }
    std::fs::create_dir_all(project.join(".infra/tools")).expect("tool defaults directory");
    std::fs::write(
        project.join(".infra/tools/defaults.toml"),
        "build_toolchain = \"1.94.0\"\n",
    )
    .expect("tool defaults");

    let inventory = scan_projects(&[project]).expect("path dependency inventory");
    let dependencies = &inventory.projects[0].dependencies;
    for (name, kind) in [
        ("normal-local", "normal"),
        ("build-local", "build"),
        ("dev-local", "dev"),
    ] {
        let dependency = dependencies.iter().find(|dependency| dependency.name == name).unwrap();
        assert_eq!(dependency.source, "path");
        assert_eq!(dependency.kind, kind);
        assert_eq!(dependency.optional, name == "normal-local");
    }
}
