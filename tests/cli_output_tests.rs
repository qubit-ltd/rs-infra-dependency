// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rs-infra-dependency"))
}

fn project_fixture() -> TempDir {
    let temporary = tempfile::tempdir().expect("temporary project");
    let source = Path::new("tests/fixtures/application-no-lock");
    for relative in [
        "Cargo.toml",
        "src/main.rs",
        ".infra/dependency/policy.toml",
        ".infra/dependency/policy/current.toml",
        ".infra/dependency/policy/baselines/v2026.09.2.toml",
    ] {
        let destination = temporary.path().join(relative);
        std::fs::create_dir_all(destination.parent().expect("parent directory")).expect("create directory");
        std::fs::copy(source.join(relative), destination).expect("copy project fixture");
    }
    temporary
}

#[cfg(unix)]
fn fake_cargo(temporary: &TempDir) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = temporary.path().join("cargo");
    std::fs::write(
        &path,
        "#!/bin/sh\nprintf '%s|%s\\n' \"${RUSTUP_TOOLCHAIN-}\" \"$*\" >> \"$CARGO_TRACE\"\nunset RUSTUP_TOOLCHAIN\nexec \"$REAL_CARGO\" \"$@\"\n",
    )
    .expect("write fake cargo");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod fake cargo");
    path
}

#[cfg(unix)]
fn command_with_fake_cargo(temporary: &TempDir) -> (Command, std::path::PathBuf) {
    let fake = fake_cargo(temporary);
    let trace = temporary.path().join("cargo-trace.txt");
    let real_cargo = std::env::var_os("CARGO").expect("Cargo test sets CARGO");
    let mut command = binary();
    command.env("CARGO", &fake);
    command.env("REAL_CARGO", real_cargo);
    command.env("CARGO_TRACE", &trace);
    command.env("CARGO_NET_OFFLINE", "true");
    command.env_remove("RUSTUP_TOOLCHAIN");
    command.env(
        "PATH",
        format!(
            "{}:{}",
            temporary.path().display(),
            std::env::var("PATH").expect("PATH")
        ),
    );
    (command, trace)
}

fn write_defaults(project: &Path, relative: &str, toolchain: &str) {
    let path = project.join(relative);
    std::fs::create_dir_all(path.parent().expect("defaults parent")).expect("create defaults directory");
    std::fs::write(path, format!("build_toolchain = \"{toolchain}\"\n")).expect("write defaults");
}

#[test]
fn test_cli_success_reports_completion() {
    let output = binary()
        .args(["inventory", "--root", "tests/fixtures/application-no-lock"])
        .output()
        .expect("run inventory command");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("✅ rs-infra-dependency: inventory succeeded"));
}

#[test]
fn test_cli_failure_reports_failure() {
    let output = binary()
        .args(["--project", "tests/fixtures/application-no-lock", "check"])
        .output()
        .expect("run check command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("❌ rs-infra-dependency: check failed"));
}

#[test]
fn test_cli_generates_and_removes_a_temporary_lockfile() {
    let project = std::path::Path::new("tests/fixtures/application-no-lock");
    let lockfile = project.join("Cargo.lock");
    assert!(!lockfile.exists());
    let output = binary()
        .args([
            "--project",
            project.to_str().expect("project path"),
            "check",
            "--temporary-lockfile",
        ])
        .output()
        .expect("run temporary-lockfile check");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(!lockfile.exists());
}

#[cfg(unix)]
#[test]
fn test_cli_uses_new_defaults_for_every_temporary_lockfile_cargo_command() {
    let temporary = project_fixture();
    let project = temporary.path();
    write_defaults(project, ".infra/tools/defaults.toml", "1.94.0");
    write_defaults(project, ".infra/ci/defaults.toml", "unused-old-toolchain");
    let (mut command, trace) = command_with_fake_cargo(&temporary);
    let output = command
        .args([
            "--project",
            project.to_str().expect("UTF-8 project"),
            "check",
            "--temporary-lockfile",
        ])
        .output()
        .expect("run policy check");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let entries = std::fs::read_to_string(trace).expect("Cargo trace");
    assert!(entries.contains("|locate-project"), "{entries}");
    assert!(entries.contains("|generate-lockfile"), "{entries}");
    assert_eq!(
        entries.lines().filter(|entry| entry.contains("|metadata")).count(),
        2,
        "{entries}"
    );
    assert!(entries.lines().all(|entry| entry.starts_with("1.94.0|")), "{entries}");
    assert!(!project.join("Cargo.lock").exists());
}

#[cfg(unix)]
#[test]
fn test_inventory_rejects_legacy_defaults_when_new_file_is_absent() {
    let temporary = project_fixture();
    let project = temporary.path();
    write_defaults(project, ".infra/ci/defaults.toml", "1.94.0");
    let (mut command, trace) = command_with_fake_cargo(&temporary);
    let output = command
        .args(["inventory", "--root", project.to_str().expect("UTF-8 project")])
        .output()
        .expect("run inventory");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&project.join(".infra/tools/defaults.toml").display().to_string()),
        "{stderr}"
    );
    assert!(!trace.exists(), "Cargo ran before required defaults were loaded");
}

#[test]
fn test_sync_does_not_require_tool_defaults() {
    let temporary = project_fixture();
    let output = binary()
        .args([
            "--project",
            temporary.path().to_str().expect("UTF-8 project"),
            "sync",
            "--dry-run",
        ])
        .output()
        .expect("run sync planning");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn test_check_reports_corrupt_new_defaults_without_legacy_fallback() {
    let temporary = project_fixture();
    let project = temporary.path();
    write_defaults(project, ".infra/ci/defaults.toml", "1.94.0");
    let new_path = project.join(".infra/tools/defaults.toml");
    std::fs::create_dir_all(new_path.parent().expect("defaults parent")).expect("create defaults directory");
    std::fs::write(&new_path, "build_toolchain = [\n").expect("write invalid defaults");
    let output = binary()
        .args([
            "--project",
            project.to_str().expect("UTF-8 project"),
            "check",
            "--temporary-lockfile",
        ])
        .output()
        .expect("run check");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(new_path.to_str().expect("UTF-8 path")), "{stderr}");
}

#[test]
fn test_inventory_reports_missing_defaults_path() {
    let temporary = project_fixture();
    let path = temporary.path().join(".infra/tools/defaults.toml");
    let output = binary()
        .args(["inventory", "--root", temporary.path().to_str().expect("UTF-8 project")])
        .output()
        .expect("run inventory");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(path.to_str().expect("UTF-8 path")), "{stderr}");
}

#[cfg(unix)]
#[test]
fn test_inventory_does_not_fall_back_from_a_broken_new_defaults_link() {
    let temporary = project_fixture();
    let project = temporary.path();
    write_defaults(project, ".infra/ci/defaults.toml", "1.94.0");
    let new_path = project.join(".infra/tools/defaults.toml");
    std::fs::create_dir_all(new_path.parent().expect("defaults parent")).expect("create defaults directory");
    std::os::unix::fs::symlink("missing.toml", &new_path).expect("create broken defaults link");
    let output = binary()
        .args(["inventory", "--root", project.to_str().expect("UTF-8 project")])
        .output()
        .expect("run inventory");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(new_path.to_str().expect("UTF-8 path")), "{stderr}");
}
