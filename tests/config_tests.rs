// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use camino::Utf8Path;
use qubit_infra_dependency::ProjectConfig;
use tempfile::tempdir;

#[test]
fn loads_shared_policy_without_project_baseline_selection() {
    let directory = tempdir().expect("temporary project");
    let project = Utf8Path::from_path(directory.path()).expect("UTF-8 project path");
    std::fs::create_dir_all(project.join(".infra/dependency")).expect("policy directory");
    std::fs::write(
        project.join(".infra/dependency/policy.toml"),
        "format = 3\ninternal-prefixes = [\"qubit-\"]\n",
    )
    .expect("shared policy");

    let config = ProjectConfig::load(project, None).expect("shared project policy");
    assert_eq!(config.format, 3);
    assert!(config.baseline.is_empty());
}

#[test]
fn rejects_project_baseline_selection_in_shared_policy() {
    let directory = tempdir().expect("temporary project");
    let project = Utf8Path::from_path(directory.path()).expect("UTF-8 project path");
    std::fs::create_dir_all(project.join(".infra/dependency")).expect("policy directory");
    std::fs::write(
        project.join(".infra/dependency/policy.toml"),
        "format = 3\nbaseline = \"v2026.09.14\"\ninternal-prefixes = [\"qubit-\"]\n",
    )
    .expect("project policy");

    let error = ProjectConfig::load(project, None).expect_err("project baseline selection must fail");
    assert_eq!(error.code(), "DP001");
}

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

#[test]
fn loads_an_explicit_configuration_path_and_reports_parse_errors() {
    let directory = tempdir().expect("temporary project");
    let project = Utf8Path::from_path(directory.path()).expect("UTF-8 project path");
    let override_path = project.join("custom-policy.toml");
    std::fs::write(&override_path, "format = 3\ninternal-prefixes = [\"qubit-\"]\n").expect("override config");
    let config = ProjectConfig::load(project, Some(&override_path)).expect("override config loads");
    assert_eq!(config.format, 3);

    std::fs::write(&override_path, "format = [\n").expect("invalid override config");
    assert!(ProjectConfig::load(project, Some(&override_path)).is_err());
}
