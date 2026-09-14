// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Local and Git baseline source resolution.

use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::process::Command;

use camino::Utf8Path;
use camino::Utf8PathBuf;
use url::Url;

use crate::Baseline;
use crate::PolicyError;
use crate::ProjectConfig;

// qubit-style: allow type-file-name

/// A baseline together with the source commit used to load it.
///
/// # Examples
///
/// Baselines are loaded from a fixed project configuration revision.
///
/// ```
/// use qubit_infra_dependency::{Baseline, LoadedBaseline};
///
/// let loaded = LoadedBaseline {
///     commit: "0123456789abcdef0123456789abcdef01234567".into(),
///     release: "v2026.09.0".into(),
///     baseline: Baseline::parse("serde 1.0\n").expect("valid baseline"),
/// };
/// assert_eq!(loaded.release, "v2026.09.0");
/// ```
#[derive(Debug, Clone)]
pub struct LoadedBaseline {
    /// Commit SHA recorded by the project configuration.
    pub commit: String,
    /// Baseline release selected by the project configuration.
    pub release: String,
    /// Parsed and validated baseline.
    pub baseline: Baseline,
}

/// Loads a baseline from a local or Git policy source pinned to its revision.
pub fn load_baseline(
    reference: &ProjectConfig,
    cache: &Utf8Path,
) -> Result<LoadedBaseline, PolicyError> {
    let source = Url::parse(&reference.source).map_err(|error| PolicyError::Source {
        message: format!("invalid policy source URL: {error}"),
    })?;
    let root = match source.scheme() {
        "file" => file_source_root(&source)?,
        "http" | "https" | "ssh" | "git+file" | "git+http" | "git+https" | "git+ssh" => {
            load_git_source(reference, cache)?
        }
        scheme => {
            return Err(PolicyError::Source {
                message: format!("unsupported source scheme {scheme}"),
            });
        }
    };
    load_baseline_from_root(reference, root)
}

/// Converts a file URL into the UTF-8 local path containing the baseline.
fn file_source_root(source: &Url) -> Result<Utf8PathBuf, PolicyError> {
    let root = source.to_file_path().map_err(|_| PolicyError::Source {
        message: "file policy source has no local path".into(),
    })?;
    Utf8PathBuf::from_path_buf(root).map_err(|_| PolicyError::Source {
        message: "policy source path is not valid UTF-8".into(),
    })
}

/// Clones or refreshes a Git source and checks out its requested revision.
fn load_git_source(
    reference: &ProjectConfig,
    cache: &Utf8Path,
) -> Result<Utf8PathBuf, PolicyError> {
    let cache = absolute_path(cache)?;
    let source = reference
        .source
        .strip_prefix("git+")
        .unwrap_or(&reference.source);
    let root = cache.join(git_cache_name(source));
    if root.exists() {
        run_git(&root, &["fetch", "--force", "--tags", "origin"])?;
    } else {
        std::fs::create_dir_all(cache.as_std_path()).map_err(|error| PolicyError::Source {
            message: format!("failed to create Git source cache {cache}: {error}"),
        })?;
        run_git_in(&cache, &["clone", "--no-checkout", source, root.as_str()])?;
    }
    run_git(
        &root,
        &["checkout", "--detach", "--force", &reference.revision],
    )?;
    let head = run_git(&root, &["rev-parse", "HEAD"])?;
    if !head.eq_ignore_ascii_case(&reference.revision) {
        return Err(PolicyError::Source {
            message: format!(
                "Git source HEAD {head} does not match requested revision {}",
                reference.revision
            ),
        });
    }
    Ok(root)
}

/// Returns an absolute UTF-8 path without requiring the path to exist.
fn absolute_path(path: &Utf8Path) -> Result<Utf8PathBuf, PolicyError> {
    if path.is_absolute() {
        return Ok(path.to_owned());
    }
    let current = std::env::current_dir().map_err(|error| PolicyError::Source {
        message: format!("failed to determine the current directory: {error}"),
    })?;
    let current = Utf8PathBuf::from_path_buf(current).map_err(|_| PolicyError::Source {
        message: "the current directory is not valid UTF-8".into(),
    })?;
    Ok(current.join(path))
}

/// Creates a stable cache directory name for a source URL.
fn git_cache_name(source: &str) -> String {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    format!("git-{:016x}", hasher.finish())
}

/// Runs Git in an existing repository and returns trimmed UTF-8 stdout.
#[inline]
fn run_git(repository: &Utf8Path, arguments: &[&str]) -> Result<String, PolicyError> {
    run_git_in(repository, arguments)
}

/// Runs Git in a directory and maps process or output failures to policy errors.
fn run_git_in(directory: &Utf8Path, arguments: &[&str]) -> Result<String, PolicyError> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .output()
        .map_err(|error| PolicyError::Source {
            message: format!("failed to execute git {}: {error}", arguments.join(" ")),
        })?;
    if !output.status.success() {
        return Err(PolicyError::Source {
            message: format!(
                "git {} failed: {}",
                arguments.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    String::from_utf8(output.stdout)
        .map(|output| output.trim().to_owned())
        .map_err(|error| PolicyError::Source {
            message: format!(
                "git {} returned non-UTF-8 output: {error}",
                arguments.join(" ")
            ),
        })
}

/// Reads and validates the selected baseline file from a source root.
fn load_baseline_from_root(
    reference: &ProjectConfig,
    root: Utf8PathBuf,
) -> Result<LoadedBaseline, PolicyError> {
    let baseline_path = root
        .join("policy/baselines")
        .join(format!("{}.txt", reference.baseline));
    let text = std::fs::read_to_string(baseline_path.as_std_path()).map_err(|error| {
        PolicyError::Baseline {
            message: format!("failed to read {}: {error}", baseline_path),
        }
    })?;
    let baseline = Baseline::parse(&text)?;
    Ok(LoadedBaseline {
        commit: reference.revision.clone(),
        release: reference.baseline.clone(),
        baseline,
    })
}
