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
use serde::Deserialize;
use sha2::Digest;
use sha2::Sha256;
use url::Url;

use crate::Baseline;
use crate::PolicyError;
use crate::ProjectConfig;

/// A baseline together with the revision or content digest used to load it.
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
    /// Git commit for a legacy source, or SHA-256 digest for a project copy.
    pub commit: String,
    /// Baseline release selected by the shared current-policy file.
    pub release: String,
    /// Parsed and validated baseline.
    pub baseline: Baseline,
}

/// Shared current-policy pointer installed beside the project baseline.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentPolicy {
    /// Release name shared by all projects after an infrastructure update.
    baseline: String,
}

/// Loads the current baseline installed inside one project.
///
/// The project has no baseline selection of its own. This reads the shared
/// `current.toml` pointer and its versioned baseline under `.infra/dependency`.
/// The returned identity contains a SHA-256 digest of the installed bytes.
///
/// # Errors
///
/// Returns a configuration or baseline error if the pointer is missing or
/// malformed, if its release name is unsafe, or if the selected file is
/// missing or invalid. No network access occurs.
pub fn load_project_baseline(project: &Utf8Path) -> Result<LoadedBaseline, PolicyError> {
    let root = project.join(".infra/dependency/policy");
    let current_path = root.join("current.toml");
    let current_text = std::fs::read_to_string(current_path.as_std_path()).map_err(|source| {
        PolicyError::ReadConfig {
            path: current_path.to_string(),
            source,
        }
    })?;
    let current: CurrentPolicy = toml::from_str(&current_text).map_err(|source| PolicyError::ParseConfig {
        path: current_path.to_string(),
        source,
    })?;
    if current.baseline.is_empty()
        || !current
            .baseline
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(PolicyError::InvalidConfig {
            code: "DP001",
            message: format!("invalid shared baseline release in {current_path}"),
        });
    }
    let directory = root.join("baselines");
    let toml_path = directory.join(format!("{}.toml", current.baseline));
    let text_path = directory.join(format!("{}.txt", current.baseline));
    let baseline_path = match (toml_path.is_file(), text_path.is_file()) {
        (true, false) => toml_path,
        (false, true) => text_path,
        (true, true) => {
            return Err(PolicyError::Baseline {
                message: format!("both {toml_path} and {text_path} exist"),
            });
        }
        (false, false) => {
            return Err(PolicyError::Baseline {
                message: format!(
                    "baseline file is missing: {toml_path} or {text_path}; run ./update-infra.sh"
                ),
            });
        }
    };
    let bytes = std::fs::read(baseline_path.as_std_path()).map_err(|error| PolicyError::Baseline {
        message: format!("failed to read {baseline_path}: {error}"),
    })?;
    let text = std::str::from_utf8(&bytes).map_err(|error| PolicyError::Baseline {
        message: format!("baseline {baseline_path} is not UTF-8: {error}"),
    })?;
    let baseline = if baseline_path.extension() == Some("toml") {
        Baseline::parse_toml(text)?
    } else {
        Baseline::parse(text)?
    };
    let digest = Sha256::digest(text.replace("\r\n", "\n").as_bytes());
    Ok(LoadedBaseline {
        commit: format!("sha256:{digest:x}"),
        release: current.baseline,
        baseline,
    })
}

/// Loads a baseline from a local or Git policy source pinned to its revision.
///
/// Local `file://` sources are read directly. Git sources are cached below
/// `cache`, fetched when already present, and checked out at the exact commit
/// recorded in `reference` before the baseline file is read.
///
/// # Errors
///
/// Returns [`PolicyError::Source`] for invalid or unsupported URLs, Git
/// failures, cache failures, or a revision mismatch. Returns
/// [`PolicyError::Baseline`] when the selected baseline cannot be read or
/// parsed.
///
/// # Parameters
///
/// * `reference` - Validated project configuration selecting the source and
///   revision.
/// * `cache` - Directory used for cached Git checkouts.
///
/// # Returns
///
/// Returns the parsed baseline and the selected source commit.
pub fn load_baseline(reference: &ProjectConfig, cache: &Utf8Path) -> Result<LoadedBaseline, PolicyError> {
    if let Some(root) = std::env::var_os("RS_INFRA_DEPENDENCY_POLICY_ROOT") {
        let root = Utf8PathBuf::from_path_buf(root.into()).map_err(|_| PolicyError::Source {
            message: "dynamic policy source path is not valid UTF-8".into(),
        })?;
        let root = absolute_path(&root)?;
        let commit = run_git(&root, &["rev-parse", "HEAD"])?;
        return load_baseline_from_root(reference, root, commit);
    }
    if reference.source.is_empty() || reference.revision.is_empty() {
        return Err(PolicyError::Source {
            message: "dynamic policy source is unavailable; run rs-infra-dependency through rs-infra-tools".into(),
        });
    }
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
    load_baseline_from_root(reference, root, reference.revision.clone())
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
fn load_git_source(reference: &ProjectConfig, cache: &Utf8Path) -> Result<Utf8PathBuf, PolicyError> {
    let cache = absolute_path(cache)?;
    let source = reference.source.strip_prefix("git+").unwrap_or(&reference.source);
    let root = cache.join(git_cache_name(source));
    if root.exists() {
        run_git(&root, &["fetch", "--force", "--tags", "origin"])?;
    } else {
        std::fs::create_dir_all(cache.as_std_path()).map_err(|error| PolicyError::Source {
            message: format!("failed to create Git source cache {cache}: {error}"),
        })?;
        run_git_in(&cache, &["clone", "--no-checkout", source, root.as_str()])?;
    }
    run_git(&root, &["checkout", "--detach", "--force", &reference.revision])?;
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

/// Runs Git in a directory and maps process or output failures to policy
/// errors.
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
            message: format!("git {} returned non-UTF-8 output: {error}", arguments.join(" ")),
        })
}

/// Reads and validates the selected baseline file from a source root.
fn load_baseline_from_root(
    reference: &ProjectConfig,
    root: Utf8PathBuf,
    commit: String,
) -> Result<LoadedBaseline, PolicyError> {
    let shared_directory = root.join("conf/policy/baselines");
    let directory = if shared_directory.is_dir() {
        shared_directory
    } else {
        root.join("policy/baselines")
    };
    let toml_path = directory.join(format!("{}.toml", reference.baseline));
    let text_path = directory.join(format!("{}.txt", reference.baseline));
    let baseline_path = match (toml_path.exists(), text_path.exists()) {
        (true, true) => {
            return Err(PolicyError::Baseline {
                message: format!("both {toml_path} and {text_path} exist"),
            });
        }
        (true, false) => toml_path,
        (false, true) => text_path,
        (false, false) => {
            return Err(PolicyError::Baseline {
                message: format!("baseline file is missing: {toml_path} or {text_path}"),
            });
        }
    };
    let text = std::fs::read_to_string(baseline_path.as_std_path()).map_err(|error| PolicyError::Baseline {
        message: format!("failed to read {}: {error}", baseline_path),
    })?;
    let baseline = if baseline_path.extension() == Some("toml") {
        Baseline::parse_toml(&text)?
    } else {
        Baseline::parse(&text)?
    };
    Ok(LoadedBaseline {
        commit,
        release: reference.baseline.clone(),
        baseline,
    })
}
