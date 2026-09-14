// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_infra_dependency::BaselineIdentity;
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
