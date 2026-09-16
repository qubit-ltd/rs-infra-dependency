// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::process::Command;

use camino::Utf8Path;
use camino::Utf8PathBuf;
use qubit_infra_dependency::ProjectConfig;
use qubit_infra_dependency::load_baseline;
use tempfile::TempDir;
use tempfile::tempdir;

fn run_git(directory: &Utf8Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("git must be available for source resolver tests");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output must be UTF-8")
        .trim()
        .to_owned()
}

fn baseline(requirement: &str) -> String {
    format!("num-bigint {requirement}\n")
}

fn create_remote() -> (TempDir, Utf8PathBuf, String) {
    let temporary = TempDir::new().expect("temporary repository");
    let root = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 temp path");
    let remote = root.join("remote.git");
    let worktree = root.join("worktree");
    std::fs::create_dir_all(worktree.join("policy/baselines").as_std_path()).expect("baseline directory");
    run_git(&root, &["init", "--bare", remote.as_str()]);
    run_git(&root, &["init", "-b", "main", worktree.as_str()]);
    run_git(&worktree, &["config", "user.email", "test@example.invalid"]);
    run_git(&worktree, &["config", "user.name", "Dependency Policy Test"]);
    std::fs::write(
        worktree.join("policy/baselines/v2026.09.0.txt").as_std_path(),
        baseline("^0.4"),
    )
    .expect("first baseline");
    run_git(&worktree, &["add", "."]);
    run_git(&worktree, &["commit", "-m", "first baseline"]);
    let revision = run_git(&worktree, &["rev-parse", "HEAD"]);
    run_git(&worktree, &["remote", "add", "origin", remote.as_str()]);
    run_git(&worktree, &["push", "origin", "main"]);

    std::fs::write(
        worktree.join("policy/baselines/v2026.09.0.txt").as_std_path(),
        baseline("^0.5"),
    )
    .expect("second baseline");
    run_git(&worktree, &["add", "."]);
    run_git(&worktree, &["commit", "-m", "second baseline"]);
    run_git(&worktree, &["push", "origin", "main"]);

    (temporary, remote, revision)
}

#[test]
fn test_load_baseline_checks_out_the_requested_git_revision() {
    let (_temporary, remote, revision) = create_remote();
    let source = format!("git+file://{remote}");
    let reference = ProjectConfig {
        format: 2,
        source,
        revision: revision.clone(),
        baseline: "v2026.09.0".into(),
        internal_prefixes: Vec::new(),
    };
    let cache = tempdir().expect("cache directory");
    let cache = Utf8Path::from_path(cache.path()).expect("UTF-8 cache path");

    let loaded = load_baseline(&reference, cache).expect("pinned Git baseline should load");

    assert_eq!(loaded.commit, revision);
    assert_eq!(
        loaded
            .baseline
            .requirement("num-bigint")
            .expect("num-bigint rule")
            .text(),
        "^0.4"
    );
}

#[test]
fn test_load_baseline_uses_a_relative_project_cache_for_git_sources() {
    let (_temporary, remote, revision) = create_remote();
    let reference = ProjectConfig {
        format: 2,
        source: format!("git+file://{remote}"),
        revision,
        baseline: "v2026.09.0".into(),
        internal_prefixes: Vec::new(),
    };
    let cache = Utf8Path::new("target/source-test-relative-cache");
    let _ = std::fs::remove_dir_all(cache.as_std_path());
    let result = load_baseline(&reference, cache);
    let _ = std::fs::remove_dir_all(cache.as_std_path());

    result.expect("relative cache path should load a pinned Git baseline");
}

#[test]
fn test_load_baseline_keeps_file_sources_compatible() {
    let source_root = Utf8Path::new("tests/fixtures/policy-repo")
        .canonicalize_utf8()
        .expect("fixture path");
    let reference = ProjectConfig {
        format: 2,
        source: format!("file://{source_root}"),
        revision: "0123456789abcdef0123456789abcdef01234567".into(),
        baseline: "v2026.09.0".into(),
        internal_prefixes: Vec::new(),
    };

    let loaded = load_baseline(&reference, Utf8Path::new("target/source-test-cache"))
        .expect("file source should remain supported");

    assert_eq!(loaded.commit, reference.revision);
}
