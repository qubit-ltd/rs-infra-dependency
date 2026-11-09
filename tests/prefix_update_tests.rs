use std::fs;
use std::process::Command;

use camino::Utf8Path;

fn write_crate(root: &Utf8Path, name: &str, version: &str) {
    fs::create_dir_all(root.join("src")).expect("crate source directory");
    fs::write(
        root.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2021\"\n"),
    )
    .expect("crate manifest");
    fs::write(root.join("src/lib.rs"), "").expect("crate source");
}

fn fake_crates_io(temp: &tempfile::TempDir, version: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let bin = temp.path().join("bin");
    fs::create_dir_all(&bin).expect("bin directory");
    let curl = bin.join("curl");
    fs::write(
        &curl,
        format!(
            "#!/bin/sh\nprintf '{{\"crate\":{{\"max_stable_version\":\"{version}\"}}}}\\n200'\n"
        ),
    )
    .expect("fake curl");
    fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).expect("executable curl");
    bin
}

#[cfg(unix)]
#[test]
fn updates_fixture_to_newer_local_crate_and_preserves_other_fields() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let root = Utf8Path::from_path(temp.path()).expect("UTF-8 temp path");
    let project = root.join("project");
    let fixture = project.join("tests/fixtures/application_consumer");
    let local_crate = root.join("qubit-fs-registry");
    fs::create_dir_all(&fixture).expect("fixture directory");
    write_crate(&project, "root-crate", "0.1.0");
    write_crate(&local_crate, "qubit-fs-registry", "0.8.0");
    fs::write(
        fixture.join("Cargo.toml"),
        "[package]\nname = \"application-consumer\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\nqubit-fs-registry = { path = \"../../../../qubit-fs-registry\", version = \"0.7\", features = [\"sync\"], optional = true }\n",
    )
    .expect("fixture manifest");
    let before = fs::read_to_string(fixture.join("Cargo.toml")).expect("read fixture manifest");
    let bin = fake_crates_io(&temp, "0.7.9");
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH"),
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_rs-infra-dependency"))
        .args(["--project"])
        .arg(project.as_str())
        .args(["update-prefix", "--prefix", "qubit-", "--dry-run"])
        .env("PATH", std::env::join_paths(paths).expect("joined PATH"))
        .output()
        .expect("run prefix updater");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("path ../../../../qubit-fs-registry"),
        "{stdout}"
    );
    assert!(stdout.contains("version = \"^0.8\""), "{stdout}");
    assert_eq!(
        fs::read_to_string(fixture.join("Cargo.toml")).expect("read fixture manifest"),
        before
    );
}

#[cfg(unix)]
#[test]
fn switches_path_dependency_to_crates_io_when_local_version_is_older() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let root = Utf8Path::from_path(temp.path()).expect("UTF-8 temp path");
    let project = root.join("project");
    let fixture = project.join("tests/fixtures/application_consumer");
    let local_crate = root.join("qubit-fs-registry");
    fs::create_dir_all(&fixture).expect("fixture directory");
    write_crate(&project, "root-crate", "0.1.0");
    write_crate(&local_crate, "qubit-fs-registry", "0.7.9");
    fs::write(
        fixture.join("Cargo.toml"),
        "[package]\nname = \"application-consumer\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\nqubit-fs-registry = { path = \"../../../../qubit-fs-registry\", version = \"0.7\", features = [\"sync\"], optional = true }\n",
    )
    .expect("fixture manifest");
    let bin = fake_crates_io(&temp, "0.8.1");
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH"),
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_rs-infra-dependency"))
        .args(["--project"])
        .arg(project.as_str())
        .args(["update-prefix", "--prefix", "qubit-"])
        .env("PATH", std::env::join_paths(paths).expect("joined PATH"))
        .output()
        .expect("run prefix updater");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(fixture.join("Cargo.toml")).expect("read fixture manifest");
    assert!(updated.contains("version = \"^0.8\""), "{updated}");
    assert!(!updated.contains("path ="), "{updated}");
    assert!(updated.contains("features = [\"sync\"]"), "{updated}");
    assert!(updated.contains("optional = true"), "{updated}");
}
