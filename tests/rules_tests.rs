// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use camino::Utf8Path;
use camino::Utf8PathBuf;
use qubit_infra_dependency::ProjectConfig;
use qubit_infra_dependency::evaluate;
use qubit_infra_dependency::load_baseline;

fn config(_project: &Utf8Path) -> ProjectConfig {
    ProjectConfig {
        format: 2,
        source: format!(
            "file://{}",
            Utf8Path::new("tests/fixtures/policy-repo")
                .canonicalize_utf8()
                .expect("policy fixture")
        ),
        revision: "0123456789abcdef0123456789abcdef01234567".into(),
        baseline: "v2026.09.0".into(),
        internal_prefixes: Vec::new(),
    }
}

#[test]
fn reports_a_direct_num_bigint_version_drift() {
    let project = Utf8PathBuf::from("tests/fixtures/num-bigint-05");
    let config = config(&project);
    let baseline = load_baseline(&config, Utf8Path::new("target/t3/cache")).expect("baseline");
    let evaluation = evaluate(&project, &config, &baseline).expect("evaluation");
    assert!(
        evaluation
            .violations
            .iter()
            .any(|item| item.code == "DP202")
    );
}

#[test]
fn accepts_an_application_without_a_lockfile() {
    let project = Utf8PathBuf::from("tests/fixtures/application-no-lock");
    let config = config(&project);
    let baseline = load_baseline(&config, Utf8Path::new("target/t3/cache")).expect("baseline");
    let evaluation = evaluate(&project, &config, &baseline).expect("evaluation");
    assert!(
        !evaluation
            .violations
            .iter()
            .any(|item| item.code == "DP201")
    );
}

#[test]
fn reports_an_external_direct_dependency_missing_from_the_baseline() {
    let temporary = tempfile::tempdir().expect("temporary project");
    let project = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 path");
    std::fs::create_dir_all(project.join("src").as_std_path()).expect("source directory");
    std::fs::write(
        project.join("Cargo.toml").as_std_path(),
        "[package]\nname = \"missing-baseline\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nserde = \"1.0\"\n",
    )
    .expect("manifest");
    std::fs::write(project.join("src/lib.rs").as_std_path(), "").expect("library source");
    let config = config(&project);
    let baseline = load_baseline(&config, Utf8Path::new("target/t3/cache")).expect("baseline");

    let evaluation = evaluate(&project, &config, &baseline).expect("evaluation");

    assert!(
        evaluation
            .violations
            .iter()
            .any(|item| item.code == "DP203")
    );
}
