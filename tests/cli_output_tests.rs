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

#[test]
fn test_legacy_cargo_subcommand_binary_remains_available() {
    let temporary = project_fixture();
    write_defaults(temporary.path(), ".infra/tools/defaults.toml", "1.94.0");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-dependency-policy"))
        .args([
            "dependency-policy",
            "inventory",
            "--root",
            temporary.path().to_str().expect("UTF-8 project"),
        ])
        .output()
        .expect("run compatibility entry point");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
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
    let temporary = project_fixture();
    write_defaults(temporary.path(), ".infra/tools/defaults.toml", "1.94.0");
    let output = binary()
        .args(["inventory", "--root", temporary.path().to_str().expect("UTF-8 project")])
        .output()
        .expect("run inventory command");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("✅ rs-infra-dependency: inventory succeeded"));
}

#[test]
fn test_cli_failure_reports_failure() {
    let temporary = project_fixture();
    write_defaults(temporary.path(), ".infra/tools/defaults.toml", "1.94.0");
    let output = binary()
        .args(["--project", temporary.path().to_str().expect("UTF-8 project"), "check"])
        .output()
        .expect("run check command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("❌ rs-infra-dependency: check failed"));
}

#[test]
fn test_cli_generates_and_removes_a_temporary_lockfile() {
    let temporary = project_fixture();
    let project = temporary.path();
    write_defaults(project, ".infra/tools/defaults.toml", "1.94.0");
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"temporary-lockfile-cli-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("dependency-free manifest");
    let lockfile = project.join("Cargo.lock");
    assert!(!lockfile.exists());
    let output = binary()
        .env("CARGO_NET_OFFLINE", "true")
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
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"temporary-lockfile-toolchain-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("dependency-free manifest");
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
fn test_cli_reports_json_and_applies_sync_plan() {
    let temporary = project_fixture();
    let project = temporary.path();
    write_defaults(project, ".infra/tools/defaults.toml", "1.94.0");
    let manifest_path = project.join("Cargo.toml");
    std::fs::write(
        &manifest_path,
        "[package]\nname = \"cli-report-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("dependency-free manifest");
    let report = binary()
        .env("CARGO_NET_OFFLINE", "true")
        .args([
            "--project",
            project.to_str().expect("UTF-8 project"),
            "report",
            "--format",
            "json",
            "--temporary-lockfile",
        ])
        .output()
        .expect("run JSON report");
    assert!(report.status.success(), "{}", String::from_utf8_lossy(&report.stderr));
    assert!(String::from_utf8_lossy(&report.stdout).contains("schema_version"));
    let markdown_report = binary()
        .env("CARGO_NET_OFFLINE", "true")
        .args([
            "--project",
            project.to_str().expect("UTF-8 project"),
            "report",
            "--format",
            "markdown",
            "--temporary-lockfile",
        ])
        .output()
        .expect("run Markdown report");
    assert!(
        markdown_report.status.success(),
        "{}",
        String::from_utf8_lossy(&markdown_report.stderr)
    );
    assert!(String::from_utf8_lossy(&markdown_report.stdout).contains("Dependency Policy Report"));

    std::fs::write(
        &manifest_path,
        "[package]\nname = \"cli-report-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\nnum-bigint = \"0.5\"\n",
    )
    .expect("out-of-policy manifest");
    let sync = binary()
        .args(["--project", project.to_str().expect("UTF-8 project"), "sync"])
        .output()
        .expect("apply sync");
    assert!(sync.status.success(), "{}", String::from_utf8_lossy(&sync.stderr));
    let updated_manifest = std::fs::read_to_string(manifest_path).expect("updated manifest");
    assert!(updated_manifest.contains("num-bigint = \"^0.4\""));
}

#[test]
fn test_inventory_writes_json_to_a_file_and_reports_write_errors() {
    let temporary = project_fixture();
    let project = temporary.path();
    write_defaults(project, ".infra/tools/defaults.toml", "1.94.0");
    let output_path = project.join("inventory.json");
    let output = binary()
        .args([
            "inventory",
            "--root",
            project.to_str().expect("UTF-8 project"),
            "--format",
            "json",
            "--output",
            output_path.to_str().expect("UTF-8 output path"),
        ])
        .output()
        .expect("write inventory JSON");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(
        std::fs::read_to_string(&output_path)
            .unwrap()
            .contains("direct_requirements")
    );

    let bad_output = project.join("missing-parent/inventory.json");
    let output = binary()
        .args([
            "inventory",
            "--root",
            project.to_str().expect("UTF-8 project"),
            "--format",
            "json",
            "--output",
            bad_output.to_str().expect("UTF-8 output path"),
        ])
        .output()
        .expect("attempt inventory write to missing parent");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("failed to read policy configuration"));
}

#[test]
fn test_check_returns_a_policy_violation_for_a_requirement_drift() {
    let temporary = project_fixture();
    let project = temporary.path();
    write_defaults(project, ".infra/tools/defaults.toml", "1.94.0");
    std::fs::write(
        project.join(".infra/dependency/policy/baselines/v2026.09.2.toml"),
        "format = 3\n[direct]\nnum-bigint = \"^0.4\"\n",
    )
    .expect("direct-only baseline");
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"policy-violation-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\nnum-bigint = \"^0.5\"\n",
    )
    .expect("drifting dependency manifest");
    let output = binary()
        .args(["--project", project.to_str().expect("UTF-8 project"), "check"])
        .output()
        .expect("run check");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("policy violation(s) found"));
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
