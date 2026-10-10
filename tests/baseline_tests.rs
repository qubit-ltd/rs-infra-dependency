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

#[test]
fn covers_baseline_iterators_and_invalid_input_paths() {
    let baseline = Baseline::parse("zeta 2\nalpha 1\n").expect("baseline");
    let entries = baseline
        .iter()
        .map(|(name, req)| (name.to_owned(), req.text().to_owned()))
        .collect::<Vec<_>>();
    assert_eq!(entries, [("alpha".into(), "1".into()), ("zeta".into(), "2".into())]);
    assert!(!baseline.has_resolved_rules());

    for input in ["serde 1.0 extra", "bad/name 1.0", "serde nope"] {
        assert!(Baseline::parse(input).is_err(), "input should be rejected: {input}");
    }
    for input in ["broken", "format = 2", "format = 4"] {
        assert!(
            Baseline::parse_toml(input).is_err(),
            "input should be rejected: {input}"
        );
    }
    for input in [
        "format = 3\n[direct]\n\"bad/name\" = \"1.0\"",
        "format = 3\n[direct]\nserde = \"nope\"",
        "format = 3\n[resolved]\nserde = \">=1.2\"",
        "format = 3\n[resolved]\nserde = \">=1.2.3+build\"",
    ] {
        assert!(
            Baseline::parse_toml(input).is_err(),
            "input should be rejected: {input}"
        );
    }
}
