// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use camino::Utf8PathBuf;
use qubit_infra_dependency::render_inventory_markdown;
use qubit_infra_dependency::scan_projects;

#[test]
fn scans_direct_requirements_and_resolved_graph() {
    let inventory =
        scan_projects(&[Utf8PathBuf::from("tests/fixtures/num-bigint-05")]).expect("inventory");
    assert_eq!(inventory.projects.len(), 1);
    assert!(inventory.direct_requirements.contains_key("num-bigint"));
    assert!(
        inventory.projects[0]
            .resolved
            .iter()
            .any(|p| p.name == "num-bigint")
    );
    assert!(render_inventory_markdown(&inventory).contains("num-bigint"));
}

#[test]
fn reports_requirement_conflicts_across_projects() {
    let inventory = scan_projects(&[
        Utf8PathBuf::from("tests/fixtures/num-bigint-05"),
        Utf8PathBuf::from("tests/fixtures/num-bigint-04"),
    ])
    .expect("inventory");
    assert!(inventory.conflicts.contains_key("num-bigint"));
}
