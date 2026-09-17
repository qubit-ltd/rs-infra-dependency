// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use camino::Utf8Path;
use qubit_infra_dependency::Baseline;
use qubit_infra_dependency::ProjectConfig;
use qubit_infra_dependency::load_baseline;

fn fixture_source(name: &str) -> String {
    let path = Utf8Path::new("tests/fixtures").join(name);
    format!("file://{}", path.canonicalize_utf8().expect("fixture path"))
}

#[test]
fn loads_a_text_baseline_with_patch_compatible_requirements() {
    let reference = ProjectConfig {
        format: 2,
        source: fixture_source("policy-repo"),
        revision: "0123456789abcdef0123456789abcdef01234567".into(),
        baseline: "v2026.09.0".into(),
        internal_prefixes: Vec::new(),
    };
    let baseline = load_baseline(&reference, Utf8Path::new("target/t2/cache")).expect("fixture baseline should load");
    assert_eq!(baseline.release, "v2026.09.0");
    assert_eq!(
        baseline
            .baseline
            .requirement("num-bigint")
            .expect("num-bigint requirement")
            .text(),
        "0.4"
    );
}

#[test]
fn rejects_an_unknown_release() {
    let reference = ProjectConfig {
        format: 2,
        source: fixture_source("policy-repo"),
        revision: "0123456789abcdef0123456789abcdef01234567".into(),
        baseline: "v2099.01.0".into(),
        internal_prefixes: Vec::new(),
    };
    let error = load_baseline(&reference, Utf8Path::new("target/t2/cache")).expect_err("unknown release should fail");
    assert_eq!(error.code(), "DP102");
}

#[test]
fn rejects_duplicate_or_malformed_text_rules() {
    let duplicate = Baseline::parse("serde 1.0\nserde 1.0\n").expect_err("duplicate package must be rejected");
    assert_eq!(duplicate.code(), "DP103");

    let malformed = Baseline::parse("serde\n").expect_err("line without a requirement must be rejected");
    assert_eq!(malformed.code(), "DP103");
}

#[test]
fn parses_direct_and_resolved_toml_rules() {
    let baseline =
        Baseline::parse_toml("format = 3\n\n[direct]\nserde = \"^1.0\"\n\n[resolved]\nrustls = \">=0.23.45\"\n")
            .expect("TOML baseline");
    assert_eq!(baseline.requirement("serde").expect("direct rule").text(), "^1.0");
    assert_eq!(
        baseline
            .resolved_iter()
            .find(|(name, _)| *name == "rustls")
            .expect("resolved rule")
            .1
            .text(),
        ">=0.23.45"
    );
}

#[test]
fn rejects_non_minimum_resolved_requirement() {
    let error = Baseline::parse_toml("format = 3\n[resolved]\nrustls = \"^0.23\"\n")
        .expect_err("caret is not a resolved minimum rule");
    assert!(error.to_string().contains(">=MAJOR.MINOR.PATCH"));
}
