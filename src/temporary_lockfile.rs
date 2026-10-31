// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Creation and cleanup of CI-only Cargo.lock files.

use std::process::Command;

use camino::Utf8Path;
use camino::Utf8PathBuf;

use crate::PolicyError;
use crate::cargo::build_toolchain;
use crate::cargo::workspace_root;

/// A lock file generated for one policy check and removed on cleanup.
pub(crate) struct TemporaryLockfile {
    path: Utf8PathBuf,
}

impl TemporaryLockfile {
    /// Ensures a lock file exists when resolved rules require one.
    pub(crate) fn prepare(project: &Utf8Path, required: bool, requested: bool) -> Result<Option<Self>, PolicyError> {
        if !required {
            return Ok(None);
        }
        let toolchain = build_toolchain(project)?;
        let root = workspace_root(project, &toolchain)?;
        let path = root.join("Cargo.lock");
        if path.exists() {
            return Ok(None);
        }
        if !requested {
            return Err(PolicyError::Cargo {
                code: "DP402",
                message: format!("resolved dependency rules require {path}; pass --temporary-lockfile in CI"),
            });
        }
        let manifest = if project.is_absolute() {
            project.join("Cargo.toml").to_owned()
        } else {
            let current = std::env::current_dir().map_err(|error| PolicyError::Cargo {
                code: "DP405",
                message: format!("failed to determine current directory: {error}"),
            })?;
            Utf8PathBuf::from_path_buf(current)
                .map_err(|_| PolicyError::Cargo {
                    code: "DP405",
                    message: "current directory is not valid UTF-8".into(),
                })?
                .join(project)
                .join("Cargo.toml")
        };
        generate_lockfile(
            &root,
            &path,
            manifest.as_str(),
            &toolchain,
            std::ffi::OsStr::new("cargo"),
        )?;
        Ok(Some(Self { path }))
    }

    /// Removes the generated lock file and reports cleanup failures.
    pub(crate) fn cleanup(self) -> Result<(), PolicyError> {
        std::fs::remove_file(self.path.as_std_path()).map_err(|error| PolicyError::Cargo {
            code: "DP405",
            message: format!("failed to remove temporary Cargo.lock {}: {error}", self.path),
        })
    }
}

fn generate_lockfile(
    root: &Utf8Path,
    path: &Utf8Path,
    manifest: &str,
    toolchain: &str,
    cargo_command: &std::ffi::OsStr,
) -> Result<(), PolicyError> {
    let output = Command::new(cargo_command)
        .env("RUSTUP_TOOLCHAIN", toolchain)
        .args(["generate-lockfile", "--manifest-path", manifest])
        .current_dir(root.as_std_path())
        .output()
        .map_err(|error| PolicyError::Cargo {
            code: "DP405",
            message: format!("failed to generate temporary Cargo.lock: {error}"),
        })?;
    if !output.status.success() || !path.exists() {
        if path.exists() {
            let _ = std::fs::remove_file(path.as_std_path());
        }
        return Err(PolicyError::Cargo {
            code: "DP405",
            message: format!(
                "temporary Cargo.lock generation failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(())
}

impl Drop for TemporaryLockfile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.path.as_std_path());
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::process::Command;

    use camino::Utf8PathBuf;

    use super::TemporaryLockfile;
    use super::generate_lockfile;

    fn project() -> (tempfile::TempDir, Utf8PathBuf) {
        let temporary = tempfile::tempdir().expect("temporary project");
        let root = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 project path");
        std::fs::create_dir_all(root.join(".infra/tools")).expect("tool defaults directory");
        std::fs::write(
            root.join(".infra/tools/defaults.toml"),
            "build_toolchain = \"1.94.0\"\n",
        )
        .expect("tool defaults");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"lockfile-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("manifest");
        std::fs::create_dir_all(root.join("src")).expect("source directory");
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("source");
        (temporary, root)
    }

    #[test]
    fn reports_lockfile_generator_process_start_failure() {
        let (_temporary, root) = project();
        let missing_cargo = root.join("missing-cargo");
        let error = generate_lockfile(
            &root,
            &root.join("Cargo.lock"),
            root.join("Cargo.toml").as_str(),
            "1.94.0",
            missing_cargo.as_std_path().as_os_str(),
        )
        .expect_err("missing Cargo executable should fail");

        assert!(error.to_string().contains("failed to generate temporary Cargo.lock"));
    }

    #[test]
    fn handles_optional_existing_requested_and_cleanup_lockfiles() {
        let (_temporary, root) = project();
        assert!(TemporaryLockfile::prepare(&root, false, false).unwrap().is_none());
        assert!(TemporaryLockfile::prepare(&root, true, false).is_err());

        std::fs::write(root.join("Cargo.lock"), "version = 4\n").expect("existing lockfile");
        assert!(TemporaryLockfile::prepare(&root, true, false).unwrap().is_none());
        std::fs::remove_file(root.join("Cargo.lock")).expect("remove preexisting lockfile");

        let lockfile = TemporaryLockfile::prepare(&root, true, true)
            .expect("generate lockfile")
            .expect("temporary lockfile");
        assert!(root.join("Cargo.lock").exists());
        lockfile.cleanup().expect("cleanup lockfile");
        assert!(!root.join("Cargo.lock").exists());
    }

    #[test]
    fn reports_cleanup_failure_after_lockfile_disappears() {
        let (_temporary, root) = project();
        let lockfile = TemporaryLockfile::prepare(&root, true, true)
            .expect("generate lockfile")
            .expect("temporary lockfile");
        std::fs::remove_file(root.join("Cargo.lock")).expect("remove lockfile");
        assert!(lockfile.cleanup().is_err());
    }

    #[test]
    fn resolves_relative_project_paths_and_reports_generation_failure() {
        let temporary = tempfile::tempdir_in("target").expect("temporary project");
        let root = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 project path");
        std::fs::create_dir_all(root.join(".infra/tools")).expect("tool defaults directory");
        std::fs::write(
            root.join(".infra/tools/defaults.toml"),
            "build_toolchain = \"1.94.0\"\n",
        )
        .expect("tool defaults");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"relative-lockfile-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("manifest");
        std::fs::create_dir_all(root.join("src")).expect("source directory");
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("source");
        let lockfile = TemporaryLockfile::prepare(&root, true, true)
            .expect("relative project lockfile")
            .expect("generated lockfile");
        lockfile.cleanup().expect("remove relative lockfile");

        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"lockfile-failure-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\ncrate-that-does-not-exist-anywhere = \"99.0.0\"\n",
        )
        .expect("manifest with unavailable dependency");
        std::fs::create_dir_all(root.join(".cargo")).expect("Cargo config directory");
        std::fs::write(root.join(".cargo/config.toml"), "[net]\noffline = true\n").expect("offline Cargo config");
        assert!(TemporaryLockfile::prepare(&root, true, true).is_err());
        assert!(!root.join("Cargo.lock").exists());
    }

    #[cfg(unix)]
    #[test]
    fn reports_when_the_current_directory_disappears_during_lockfile_preparation() {
        const CHILD_TEST: &str =
            "temporary_lockfile::tests::reports_when_the_current_directory_disappears_during_lockfile_preparation";
        const CHILD_MODE: &str = "RS_INFRA_TEST_REMOVED_CURRENT_DIRECTORY";

        if std::env::var_os(CHILD_MODE).is_some() {
            let project = Utf8PathBuf::from("fixture");
            let error = match TemporaryLockfile::prepare(&project, true, true) {
                Ok(_) => panic!("removed current directory should fail"),
                Err(error) => error,
            };
            assert_eq!(error.code(), "DP405");
            assert!(error.to_string().contains("failed to determine current directory"));
            return;
        }

        let temporary = tempfile::tempdir().expect("temporary working directory");
        let root = temporary.path();
        std::fs::create_dir_all(root.join("fixture/.infra/tools")).expect("fixture config directory");
        std::fs::write(
            root.join("fixture/.infra/tools/defaults.toml"),
            "build_toolchain = \"1.94.0\"\n",
        )
        .expect("tool defaults");
        std::fs::write(
            root.join("fixture/Cargo.toml"),
            "[package]\nname = \"lockfile-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("manifest");

        let bin = root.join("bin");
        std::fs::create_dir(&bin).expect("fake Cargo directory");
        let fake_cargo = bin.join("cargo");
        std::fs::write(
            &fake_cargo,
            "#!/bin/sh\nprintf '%s\\n' \"$RS_INFRA_TEST_FIXTURE_ROOT/fixture/Cargo.toml\"\n/bin/rm -rf -- \"$RS_INFRA_TEST_FIXTURE_ROOT\"\n",
        )
        .expect("fake Cargo command");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_cargo, std::fs::Permissions::from_mode(0o755)).expect("make Cargo executable");

        let output = Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", CHILD_TEST])
            .env(CHILD_MODE, "1")
            .env("RS_INFRA_TEST_FIXTURE_ROOT", root)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .current_dir(root)
            .output()
            .expect("run isolated current-directory test");
        assert!(
            output.status.success(),
            "child test failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
