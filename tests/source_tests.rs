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
use qubit_infra_dependency::load_project_baseline;
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

#[test]
fn test_project_baseline_uses_installed_current_policy() {
    let temporary = tempdir().expect("temporary project");
    let project = Utf8Path::from_path(temporary.path()).expect("UTF-8 project path");
    let policy = project.join(".infra/dependency/policy");
    std::fs::create_dir_all(policy.join("baselines")).expect("baseline directory");
    std::fs::write(policy.join("current.toml"), "baseline = \"v2026.10.06.1\"\n").expect("current policy");
    std::fs::write(policy.join("baselines/v2026.10.06.1.txt"), baseline("^0.4")).expect("installed baseline");

    let loaded = load_project_baseline(project).expect("installed project baseline");
    assert_eq!(loaded.release, "v2026.10.06.1");
    assert!(loaded.commit.starts_with("sha256:"));
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
fn rejects_invalid_installed_baseline_pointers_and_files() {
    let temporary = tempdir().expect("temporary project");
    let project = Utf8Path::from_path(temporary.path()).expect("UTF-8 project path");
    let policy = project.join(".infra/dependency/policy");
    assert!(load_project_baseline(project).is_err());
    std::fs::create_dir_all(policy.join("baselines")).expect("baseline directory");

    for current in ["not = [toml", "baseline = \"../escape\"\n", "baseline = \"\"\n"] {
        std::fs::write(policy.join("current.toml"), current).expect("current policy");
        assert!(
            load_project_baseline(project).is_err(),
            "current policy should fail: {current}"
        );
    }

    std::fs::write(policy.join("current.toml"), "baseline = \"v1\"\n").expect("current policy");
    let baselines = policy.join("baselines");
    assert!(load_project_baseline(project).is_err(), "missing baseline should fail");
    std::fs::create_dir(baselines.join("v1.txt")).expect("directory in place of baseline file");
    assert!(
        load_project_baseline(project).is_err(),
        "unreadable baseline should fail"
    );
    std::fs::remove_dir(baselines.join("v1.txt")).expect("remove baseline directory");
    std::fs::write(baselines.join("v1.txt"), b"\xff").expect("non UTF-8 baseline");
    assert!(
        load_project_baseline(project).is_err(),
        "non UTF-8 baseline should fail"
    );
    std::fs::write(baselines.join("v1.txt"), "not-a-valid-rule\n").expect("invalid baseline");
    assert!(
        load_project_baseline(project).is_err(),
        "invalid text baseline should fail"
    );
    std::fs::write(baselines.join("v1.toml"), "format = 3\n").expect("second baseline format");
    assert!(
        load_project_baseline(project).is_err(),
        "ambiguous baseline formats should fail"
    );
}

#[test]
fn rejects_an_unreadable_baseline_from_a_local_policy_source() {
    let temporary = tempdir().expect("temporary policy source");
    let root = Utf8Path::from_path(temporary.path()).expect("UTF-8 source path");
    std::fs::create_dir_all(root.join("policy/baselines")).expect("baseline directory");
    std::fs::create_dir(root.join("policy/baselines/v1.txt")).expect("directory in place of baseline");
    let reference = ProjectConfig {
        format: 2,
        source: format!("file://{root}"),
        revision: "0123456789abcdef0123456789abcdef01234567".into(),
        baseline: "v1".into(),
        internal_prefixes: Vec::new(),
    };
    let cache = Utf8Path::new("target/source-unreadable-cache");

    let error = load_baseline(&reference, cache).expect_err("baseline directory cannot be read as text");

    assert!(error.to_string().contains("failed to read"));
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
    run_git(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);

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
    let refreshed = load_baseline(&reference, cache).expect("cached Git baseline should refresh");
    assert_eq!(refreshed.commit, revision);
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
fn rejects_a_git_ref_whose_resolved_head_does_not_equal_the_requested_revision() {
    let (_temporary, remote, _) = create_remote();
    let reference = ProjectConfig {
        format: 2,
        source: format!("git+file://{remote}"),
        revision: "main".into(),
        baseline: "v2026.09.0".into(),
        internal_prefixes: Vec::new(),
    };
    let cache = tempdir().expect("cache directory");
    let cache = Utf8Path::from_path(cache.path()).expect("UTF-8 cache path");
    let error = load_baseline(&reference, cache).expect_err("branch name must not match a commit SHA");
    assert!(error.to_string().contains("does not match requested revision"));
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

#[test]
fn rejects_missing_invalid_and_unsupported_policy_sources() {
    let cache = Utf8Path::new("target/source-test-errors");
    let config = |source: &str| ProjectConfig {
        format: 2,
        source: source.into(),
        revision: "0123456789abcdef0123456789abcdef01234567".into(),
        baseline: "v2026.09.0".into(),
        internal_prefixes: Vec::new(),
    };
    for reference in [
        config(""),
        config("not a url"),
        config("data:text/plain,policy"),
        config("file:///path/that/does/not/exist"),
    ] {
        assert!(load_baseline(&reference, cache).is_err());
    }

    let temporary = tempdir().expect("cache parent");
    let cache_file = temporary.path().join("cache-file");
    std::fs::write(&cache_file, "not a directory").expect("cache path file");
    let source = ProjectConfig {
        format: 2,
        source: "git+file:///source-that-is-never-reached".into(),
        revision: "0123456789abcdef0123456789abcdef01234567".into(),
        baseline: "v1".into(),
        internal_prefixes: Vec::new(),
    };
    let cache_file = Utf8Path::from_path(&cache_file).expect("UTF-8 cache path");
    assert!(load_baseline(&source, cache_file).is_err());
}
