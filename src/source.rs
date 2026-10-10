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
    let current_text =
        std::fs::read_to_string(current_path.as_std_path()).map_err(|source| PolicyError::ReadConfig {
            path: current_path.to_string(),
            source,
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
                message: format!("baseline file is missing: {toml_path} or {text_path}; run ./update-infra.sh"),
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
    absolute_path_from_current_dir(path, std::env::current_dir())
}

fn absolute_path_from_current_dir(
    path: &Utf8Path,
    current: std::io::Result<std::path::PathBuf>,
) -> Result<Utf8PathBuf, PolicyError> {
    let current = current.map_err(|error| PolicyError::Source {
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
    run_git_in_with_executable(directory, arguments, std::ffi::OsStr::new("git"))
}

fn run_git_in_with_executable(
    directory: &Utf8Path,
    arguments: &[&str],
    executable: &std::ffi::OsStr,
) -> Result<String, PolicyError> {
    let output = Command::new(executable)
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

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::sync::Mutex;

    use camino::Utf8Path;
    use camino::Utf8PathBuf;

    static POLICY_ROOT_ENV: Mutex<()> = Mutex::new(());

    use super::absolute_path;
    use super::absolute_path_from_current_dir;
    use super::file_source_root;
    use super::git_cache_name;
    use super::load_baseline_from_root;
    use super::load_project_baseline;
    use super::run_git;
    use super::run_git_in;
    use super::run_git_in_with_executable;
    use crate::ProjectConfig;

    fn reference() -> ProjectConfig {
        ProjectConfig {
            format: 2,
            source: String::new(),
            revision: String::new(),
            baseline: "v1".into(),
            internal_prefixes: Vec::new(),
        }
    }

    #[test]
    fn resolves_baseline_layouts_and_failures() {
        let temporary = tempfile::tempdir().expect("temporary policy");
        let root = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 path");
        let baseline_dir = root.join("conf/policy/baselines");
        std::fs::create_dir_all(&baseline_dir).expect("baseline directory");
        std::fs::write(baseline_dir.join("v1.toml"), "format = 3\n[direct]\nserde = \"1.0\"\n").expect("TOML baseline");
        let loaded = load_baseline_from_root(&reference(), root.clone(), "rev".into()).expect("TOML baseline load");
        assert_eq!(loaded.commit, "rev");

        std::fs::remove_file(baseline_dir.join("v1.toml")).expect("remove first baseline");
        std::fs::write(baseline_dir.join("v1.txt"), "serde 1.0\n").expect("text baseline");
        assert!(load_baseline_from_root(&reference(), root.clone(), "rev".into()).is_ok());
        std::fs::write(baseline_dir.join("v1.toml"), "format = 3\n").expect("second format");
        assert!(load_baseline_from_root(&reference(), root.clone(), "rev".into()).is_err());
        std::fs::remove_file(baseline_dir.join("v1.txt")).expect("remove text baseline");
        std::fs::remove_file(baseline_dir.join("v1.toml")).expect("remove TOML baseline");
        assert!(load_baseline_from_root(&reference(), root, "rev".into()).is_err());

        let legacy_root = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 path");
        let legacy_baselines = legacy_root.join("policy/baselines");
        std::fs::create_dir_all(&legacy_baselines).expect("legacy baseline directory");
        std::fs::create_dir(legacy_baselines.join("v1.txt")).expect("unreadable baseline path");
        assert!(load_baseline_from_root(&reference(), legacy_root.clone(), "rev".into()).is_err());
        std::fs::remove_dir(legacy_baselines.join("v1.txt")).expect("remove directory baseline");
        std::fs::write(legacy_baselines.join("v1.txt"), b"\xff").expect("invalid UTF-8 baseline");
        assert!(load_baseline_from_root(&reference(), legacy_root, "rev".into()).is_err());
    }

    #[test]
    fn loads_the_installed_project_baseline_and_rejects_ambiguous_formats() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let project = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 project path");
        let policy = project.join(".infra/dependency/policy");
        let baselines = policy.join("baselines");
        std::fs::create_dir_all(&baselines).expect("baseline directory");
        assert!(load_project_baseline(&project).is_err(), "missing pointer should fail");
        std::fs::write(policy.join("current.toml"), "not = [valid toml").expect("invalid pointer");
        assert!(load_project_baseline(&project).is_err(), "invalid pointer should fail");
        std::fs::write(policy.join("current.toml"), "baseline = \"v1\"\n").expect("current pointer");
        std::fs::write(baselines.join("v1.txt"), "serde 1.0\n").expect("text baseline");

        let loaded = load_project_baseline(&project).expect("installed baseline");
        assert_eq!(loaded.release, "v1");
        assert!(loaded.commit.starts_with("sha256:"));
        assert_eq!(loaded.baseline.requirement("serde").expect("serde rule").text(), "1.0");

        std::fs::write(policy.join("current.toml"), "baseline = \"../escape\"\n").expect("unsafe pointer");
        assert!(
            load_project_baseline(&project).is_err(),
            "unsafe release name should fail"
        );
        std::fs::write(policy.join("current.toml"), "baseline = \"\"\n").expect("empty pointer");
        assert!(
            load_project_baseline(&project).is_err(),
            "empty release name should fail"
        );
        std::fs::write(policy.join("current.toml"), "baseline = \"v1\"\n").expect("restore pointer");

        std::fs::write(baselines.join("v1.toml"), "format = 3\n[direct]\nserde = \"1.0\"\n").expect("TOML baseline");
        let error = load_project_baseline(&project).expect_err("two baseline formats are ambiguous");
        assert!(error.to_string().contains("both"));
    }

    #[test]
    fn loads_a_baseline_from_a_pinned_local_git_source() {
        let temporary = tempfile::tempdir().expect("temporary repositories");
        let root = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 root");
        let remote = root.join("remote.git");
        let working = root.join("working");
        std::fs::create_dir_all(working.join("conf/policy/baselines")).expect("baseline directory");
        run_git(&root, &["init", "--bare", remote.as_str()]).expect("bare repository");
        run_git(&root, &["init", "-b", "main", working.as_str()]).expect("working repository");
        run_git(&working, &["config", "user.email", "test@example.invalid"]).expect("Git email");
        run_git(&working, &["config", "user.name", "Source Tests"]).expect("Git user");
        std::fs::write(working.join("conf/policy/baselines/v1.txt"), "serde 1.0\n").expect("baseline");
        run_git(&working, &["add", "."]).expect("stage baseline");
        run_git(&working, &["commit", "-m", "add baseline"]).expect("commit baseline");
        let revision = run_git(&working, &["rev-parse", "HEAD"]).expect("baseline revision");
        run_git(&working, &["remote", "add", "origin", remote.as_str()]).expect("remote");
        run_git(&working, &["push", "origin", "main"]).expect("push baseline");

        let reference = ProjectConfig {
            source: format!("git+file://{remote}"),
            revision: revision.clone(),
            ..reference()
        };
        let cache = root.join("cache");
        let loaded = super::load_baseline(&reference, &cache).expect("pinned Git source");
        assert_eq!(loaded.commit, revision);
        assert_eq!(loaded.baseline.requirement("serde").expect("serde rule").text(), "1.0");
    }

    #[test]
    fn covers_source_path_and_git_helpers() {
        assert!(file_source_root(&url::Url::parse("https://example.invalid").unwrap()).is_err());
        assert!(file_source_root(&url::Url::parse("file:///tmp/%FF").unwrap()).is_err());
        assert!(absolute_path(Utf8Path::new("relative-cache")).unwrap().is_absolute());
        assert_eq!(absolute_path(Utf8Path::new("/tmp")).unwrap(), Utf8Path::new("/tmp"));
        assert_eq!(
            git_cache_name("https://example.invalid/policy"),
            git_cache_name("https://example.invalid/policy")
        );
        assert!(run_git(Utf8Path::new("."), &["rev-parse", "--show-toplevel"]).is_ok());
        assert!(run_git(Utf8Path::new("."), &["not-a-git-command"]).is_err());

        let missing_git = run_git_in_with_executable(
            Utf8Path::new("."),
            &["--version"],
            Utf8Path::new("missing-git").as_std_path().as_os_str(),
        )
        .expect_err("missing Git executable should fail");
        assert!(missing_git.to_string().contains("failed to execute git"));

        let current_directory_error = absolute_path_from_current_dir(
            Utf8Path::new("relative-cache"),
            Err(std::io::Error::other("current directory unavailable")),
        )
        .expect_err("current-directory failure should be reported");
        assert!(current_directory_error.to_string().contains("current directory"));

        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;

            let non_utf8_current_directory = std::path::PathBuf::from(std::ffi::OsString::from_vec(vec![0xff]));
            let error = absolute_path_from_current_dir(Utf8Path::new("relative-cache"), Ok(non_utf8_current_directory))
                .expect_err("non-UTF-8 current directory should be rejected");
            assert!(error.to_string().contains("not valid UTF-8"));
        }
    }

    #[test]
    fn reports_non_utf8_git_output() {
        let temporary = tempfile::tempdir().expect("temporary repository");
        let root = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 repository path");
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "test@example.invalid"],
            vec!["config", "user.name", "Source Tests"],
        ] {
            run_git(&root, &args).expect("configure local repository");
        }
        std::fs::write(root.join("binary"), [0xff]).expect("binary Git file");
        run_git(&root, &["add", "binary"]).expect("stage binary file");
        run_git(&root, &["commit", "-m", "add binary"]).expect("commit binary file");

        let error = run_git_in(&root, &["show", "HEAD:binary"]).expect_err("binary output is not UTF-8");
        assert!(error.to_string().contains("non-UTF-8 output"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_non_utf8_dynamic_policy_root() {
        use std::os::unix::ffi::OsStringExt;

        let _environment_lock = POLICY_ROOT_ENV.lock().unwrap();
        let previous = std::env::var_os("RS_INFRA_DEPENDENCY_POLICY_ROOT");
        // SAFETY: this test holds the source environment lock while setting and
        // restoring the variable.
        unsafe {
            std::env::set_var(
                "RS_INFRA_DEPENDENCY_POLICY_ROOT",
                std::ffi::OsString::from_vec(vec![0xff]),
            );
        }
        let result = super::load_baseline(&reference(), Utf8Path::new("unused-cache"));
        match previous {
            Some(value) => {
                // SAFETY: the same test lock is held while restoring the variable.
                unsafe { std::env::set_var("RS_INFRA_DEPENDENCY_POLICY_ROOT", value) };
            }
            None => {
                // SAFETY: the same test lock is held while restoring the variable.
                unsafe { std::env::remove_var("RS_INFRA_DEPENDENCY_POLICY_ROOT") };
            }
        }

        assert!(
            result
                .expect_err("non-UTF-8 source path should fail")
                .to_string()
                .contains("not valid UTF-8")
        );
    }

    #[test]
    fn loads_policy_from_the_dynamic_git_root() {
        let _environment_lock = POLICY_ROOT_ENV.lock().unwrap();
        let temporary = tempfile::tempdir().expect("temporary policy repository");
        let root = Utf8PathBuf::from_path_buf(temporary.path().to_owned()).expect("UTF-8 path");
        let baseline_dir = root.join("conf/policy/baselines");
        std::fs::create_dir_all(&baseline_dir).expect("baseline directory");
        std::fs::write(baseline_dir.join("v1.txt"), "serde 1.0\n").expect("baseline");
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "test@example.invalid"],
            vec!["config", "user.name", "Coverage Test"],
            vec!["add", "."],
            vec!["commit", "-m", "baseline"],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(root.as_std_path())
                .output()
                .expect("run git");
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        }
        let revision = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(root.as_std_path())
            .output()
            .expect("read revision");
        let revision = String::from_utf8(revision.stdout).expect("UTF-8 revision");
        let previous = std::env::var_os("RS_INFRA_DEPENDENCY_POLICY_ROOT");
        // SAFETY: this test serializes access to the process variable and restores its
        // prior value.
        unsafe { std::env::set_var("RS_INFRA_DEPENDENCY_POLICY_ROOT", root.as_str()) };
        let result = super::load_baseline(&reference(), Utf8Path::new("unused-cache"));
        match previous {
            Some(value) => {
                // SAFETY: the same test lock is held while restoring the variable.
                unsafe { std::env::set_var("RS_INFRA_DEPENDENCY_POLICY_ROOT", value) };
            }
            None => {
                // SAFETY: the same test lock is held while restoring the variable.
                unsafe { std::env::remove_var("RS_INFRA_DEPENDENCY_POLICY_ROOT") };
            }
        }
        let loaded = result.expect("dynamic source baseline");
        assert_eq!(loaded.commit, revision.trim());
        assert_eq!(loaded.release, "v1");
    }
}
