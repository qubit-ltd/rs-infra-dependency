// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use camino::Utf8Path;
use qubit_infra_dependency::Baseline;
use qubit_infra_dependency::plan_sync;

fn baseline() -> Baseline {
    Baseline::parse("num-bigint 0.4\n").expect("valid text baseline")
}

#[test]
fn plans_num_bigint_version_replacement_without_writing() {
    let plan = plan_sync(
        Utf8Path::new("tests/fixtures/sync-version-only"),
        &baseline(),
    )
    .expect("sync plan");
    assert_eq!(plan.manifest_edits.len(), 1);
    assert_eq!(plan.manifest_edits[0].dependency, "num-bigint");
    assert_eq!(plan.manifest_edits[0].new, "0.4");
}

#[test]
fn blocks_inline_dependency_declarations() {
    let plan = plan_sync(
        Utf8Path::new("tests/fixtures/sync-feature-conflict"),
        &baseline(),
    )
    .expect("sync plan");
    assert_eq!(plan.manifest_edits[0].new, "0.4");
}
