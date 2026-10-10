// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_infra_dependency::BaselineIdentity;
use qubit_infra_dependency::Evaluation;
use qubit_infra_dependency::LoadedBaseline;
use qubit_infra_dependency::PolicyError;
use qubit_infra_dependency::ProjectConfig;
use qubit_infra_dependency::Report;
use qubit_infra_dependency::Violation;
use qubit_infra_dependency::render_json;
use qubit_infra_dependency::render_markdown;

#[test]
fn renders_empty_json_and_markdown_reports() {
    let report = Report {
        schema_version: 1,
        baseline: BaselineIdentity {
            release: "v2026.09.0".into(),
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
            name: "test".into(),
        },
        violations: Vec::new(),
        packages: Vec::new(),
    };
    let json = render_json(&report).expect("JSON report");
    assert!(json.contains("\"violations\": []"));
    assert!(render_markdown(&report).contains("No violations found."));
}

#[test]
fn renders_violation_code_in_markdown() {
    let report = Report {
        schema_version: 1,
        baseline: BaselineIdentity {
            release: "v2026.09.0".into(),
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
            name: "test".into(),
        },
        violations: vec![Violation {
            code: "DP204",
            crate_name: "num-bigint".into(),
            message: "forbidden".into(),
            exception_id: None,
        }],
        packages: Vec::new(),
    };
    assert!(render_markdown(&report).contains("DP204"));
}

#[test]
fn exposes_stable_codes_and_messages_for_each_error_kind() {
    let parse_error = toml::from_str::<ProjectConfig>("invalid").expect_err("invalid TOML");
    let errors = vec![
        PolicyError::ReadConfig {
            path: "p".into(),
            source: std::io::Error::other("read"),
        },
        PolicyError::ParseConfig {
            path: "p".into(),
            source: parse_error,
        },
        PolicyError::InvalidConfig {
            code: "DP001",
            message: "invalid".into(),
        },
        PolicyError::Source {
            message: "source".into(),
        },
        PolicyError::Baseline {
            message: "baseline".into(),
        },
        PolicyError::InvalidBaseline {
            message: "invalid baseline".into(),
        },
        PolicyError::Cargo {
            code: "DP201",
            message: "cargo".into(),
        },
        PolicyError::Report {
            message: "report".into(),
        },
        PolicyError::PolicyViolation { count: 2 },
        PolicyError::Unsupported {
            message: "unsupported".into(),
        },
        PolicyError::Sync { message: "sync".into() },
    ];
    let codes = [
        "DP001", "DP001", "DP001", "DP101", "DP102", "DP103", "DP201", "DP104", "DP200", "DP105", "DP301",
    ];
    for (error, expected) in errors.iter().zip(codes) {
        assert_eq!(error.code(), expected);
        assert!(!error.to_string().is_empty());
    }
}

#[test]
fn constructs_report_identity_from_the_loaded_baseline() {
    let baseline = LoadedBaseline {
        commit: "deadbeef".into(),
        release: "v1".into(),
        baseline: qubit_infra_dependency::Baseline::parse("serde 1.0\n").unwrap(),
    };
    let report = Report::from_evaluation(
        Evaluation {
            violations: Vec::new(),
            packages: Vec::new(),
        },
        &baseline,
    );
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.baseline.release, "v1");
    assert_eq!(report.baseline.name, "v1");
    assert_eq!(report.baseline.revision, "deadbeef");
}
