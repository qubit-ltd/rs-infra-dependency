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
        let root = workspace_root(project)?;
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
        let output = Command::new("cargo")
            .args(["generate-lockfile", "--manifest-path", manifest.as_str()])
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

impl Drop for TemporaryLockfile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.path.as_std_path());
    }
}
