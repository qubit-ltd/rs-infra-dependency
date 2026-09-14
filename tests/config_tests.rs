// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use camino::Utf8Path;
use qubit_infra_dependency::ProjectConfig;

#[test]
fn rejects_policy_without_a_40_digit_revision() {
    let project = Utf8Path::new("tests/fixtures/config-invalid-revision");
    let error = ProjectConfig::load(project, None).expect_err("invalid revision must fail");
    assert_eq!(error.code(), "DP001");
}

#[test]
fn loads_the_default_project_configuration() {
    let project = Utf8Path::new("tests/fixtures/config-valid");
    let config = ProjectConfig::load(project, None).expect("valid project configuration");
    assert_eq!(config.format, 2);
    assert_eq!(config.baseline, "v2026.09.0");
}

#[test]
fn reports_missing_configuration_file() {
    let error = ProjectConfig::load(Utf8Path::new("tests/fixtures/missing"), None)
        .expect_err("missing configuration must fail");
    assert_eq!(error.code(), "DP001");
}
