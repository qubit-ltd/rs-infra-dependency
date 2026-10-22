// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Command-line argument types and command execution.

use camino::Utf8PathBuf;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;

use crate::PolicyError;
use crate::ProjectConfig;
use crate::Report;
use crate::apply_sync;
use crate::evaluate;
use crate::load_project_baseline;
use crate::plan_sync;
use crate::render_inventory_json;
use crate::render_inventory_markdown;
use crate::render_json;
use crate::render_markdown;
use crate::scan_projects;
use crate::temporary_lockfile::TemporaryLockfile;

/// Runs the command-line interface for either the native binary or the
/// compatibility Cargo subcommand.
///
/// This function parses `arguments`, executes the selected operation, and
/// writes command output or diagnostics to the process streams. On an error it
/// terminates the process with status `1` for policy violations and selected
/// source errors, or status `2` for other command failures.
///
/// # Parameters
///
/// * `arguments` - Process arguments, including the executable name at index
///   zero.
pub fn run_cli(mut arguments: Vec<std::ffi::OsString>) {
    if arguments.get(1).is_some_and(|argument| argument == "dependency-policy") {
        arguments.remove(1);
    }
    let cli = Cli::parse_from(arguments);
    let command = cli.command_name();
    match cli.execute() {
        Ok(()) => eprintln!("✅ rs-infra-dependency: {command} succeeded"),
        Err(error) => {
            eprintln!("{error}");
            eprintln!("❌ rs-infra-dependency: {command} failed");
            std::process::exit(if matches!(error.code(), "DP001" | "DP101") {
                1
            } else {
                2
            });
        }
    }
}

/// Command-line arguments for rs-infra-dependency.
///
/// # Examples
///
/// ```
/// use camino::Utf8PathBuf;
/// use qubit_infra_dependency::cli::{Cli, Command};
///
/// let cli = Cli {
///     project: Utf8PathBuf::from("."),
///     config: None,
///     recursive: false,
///     temporary_lockfile: false,
///     command: Command::Check,
/// };
/// assert_eq!(cli.project, Utf8PathBuf::from("."));
/// ```
#[derive(Debug, Parser)]
#[command(name = "rs-infra-dependency")]
pub struct Cli {
    /// Project root to inspect.
    #[arg(long, default_value = ".")]
    pub project: Utf8PathBuf,
    /// Explicit project configuration path.
    #[arg(long)]
    pub config: Option<Utf8PathBuf>,
    /// Include Cargo manifests in nested crates for check and sync.
    #[arg(long, global = true)]
    pub recursive: bool,
    /// Generate and remove a lock file when resolved rules need one.
    #[arg(long, global = true)]
    pub temporary_lockfile: bool,
    /// Policy operation.
    #[command(subcommand)]
    pub command: Command,
}

/// Supported policy operations.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::cli::Command;
///
/// let command = Command::Check;
/// assert!(matches!(command, Command::Check));
/// ```
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Inventory direct declarations and resolved graphs across project roots.
    Inventory {
        /// Project roots to scan. Repeat the option for multiple repositories.
        #[arg(long = "root", required = true, num_args = 1..)]
        roots: Vec<Utf8PathBuf>,
        /// Inventory serialization format.
        #[arg(long, value_enum, default_value_t = ReportFormat::Markdown)]
        format: ReportFormat,
        /// Optional output file; stdout is used when omitted.
        #[arg(long)]
        output: Option<Utf8PathBuf>,
    },
    /// Check a project against its selected baseline.
    Check,
    /// Render the policy report.
    Report {
        /// Report serialization format.
        #[arg(long, value_enum, default_value_t = ReportFormat::Markdown)]
        format: ReportFormat,
    },
    /// Plan or apply safe dependency-version edits.
    Sync {
        /// Print edits without writing files.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Output formats supported by report.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::cli::ReportFormat;
///
/// assert!(matches!(ReportFormat::Json, ReportFormat::Json));
/// ```
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ReportFormat {
    /// Human-readable Markdown.
    Markdown,
    /// Machine-readable JSON.
    Json,
}

impl Cli {
    /// Returns the stable command name used in completion diagnostics.
    fn command_name(&self) -> &'static str {
        match &self.command {
            Command::Inventory { .. } => "inventory",
            Command::Check => "check",
            Command::Report { .. } => "report",
            Command::Sync { .. } => "sync",
        }
    }

    /// Loads and validates the selected project configuration.
    ///
    /// # Errors
    ///
    /// Returns a [`PolicyError`] when the configuration file cannot be read,
    /// parsed, or validated.
    ///
    /// # Returns
    ///
    /// Returns the validated project configuration.
    pub fn load_config(&self) -> Result<ProjectConfig, PolicyError> {
        ProjectConfig::load(&self.project, self.config.as_deref())
    }

    /// Executes the selected command and writes successful output to stdout.
    ///
    /// # Errors
    ///
    /// Returns a [`PolicyError`] when configuration, baseline loading,
    /// evaluation, rendering, or synchronization fails. A failed policy check
    /// is returned as [`PolicyError::PolicyViolation`].
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after the command completes successfully.
    pub fn execute(&self) -> Result<(), PolicyError> {
        if let Command::Inventory { roots, format, output } = &self.command {
            let inventory = scan_projects(roots)?;
            let text = match format {
                ReportFormat::Json => render_inventory_json(&inventory)?,
                ReportFormat::Markdown => render_inventory_markdown(&inventory),
            };
            if let Some(path) = output {
                std::fs::write(path.as_std_path(), format!("{text}\n")).map_err(|source| PolicyError::ReadConfig {
                    path: path.to_string(),
                    source,
                })?;
            } else {
                println!("{text}");
            }
            return Ok(());
        }
        let config = self.load_config()?;
        let baseline = load_project_baseline(&self.project)?;
        let needs_lock =
            baseline.baseline.has_resolved_rules() && matches!(self.command, Command::Check | Command::Report { .. });
        let projects = if self.recursive && matches!(self.command, Command::Check | Command::Sync { .. }) {
            discover_projects(&self.project)?
        } else {
            vec![self.project.clone()]
        };
        let result = match &self.command {
            Command::Inventory { .. } => unreachable!("inventory handled before baseline loading"),
            Command::Check => {
                let mut violations = 0;
                for project in &projects {
                    let temporary_lockfile =
                        TemporaryLockfile::prepare(project, needs_lock, self.temporary_lockfile)?;
                    let evaluation = evaluate(project, &config, &baseline)?;
                    for violation in &evaluation.violations {
                        eprintln!("{}: [{}] {}: {}", project, violation.code, violation.crate_name, violation.message);
                    }
                    violations += evaluation.violations.len();
                    if let Some(lockfile) = temporary_lockfile {
                        lockfile.cleanup()?;
                    }
                }
                if violations == 0 {
                    Ok(())
                } else {
                    Err(PolicyError::PolicyViolation { count: violations })
                }
            }
            Command::Report { format } => {
                let temporary_lockfile =
                    TemporaryLockfile::prepare(&self.project, needs_lock, self.temporary_lockfile)?;
                let evaluation = evaluate(&self.project, &config, &baseline)?;
                let report = Report::from_evaluation(evaluation, &baseline);
                let text = match format {
                    ReportFormat::Json => render_json(&report)?,
                    ReportFormat::Markdown => render_markdown(&report),
                };
                println!("{text}");
                if let Some(lockfile) = temporary_lockfile {
                    lockfile.cleanup()?;
                }
                Ok(())
            }
            Command::Sync { dry_run } => {
                let mut plan = crate::SyncPlan {
                    manifest_edits: Vec::new(),
                    lock_updates: Vec::new(),
                    blocked: Vec::new(),
                };
                for project in &projects {
                    let next = plan_sync(project, &baseline.baseline)?;
                    plan.manifest_edits.extend(next.manifest_edits);
                    plan.lock_updates.extend(next.lock_updates);
                    plan.blocked.extend(next.blocked);
                }
                for edit in &plan.manifest_edits {
                    println!("{}: {}: {} -> {}", edit.path, edit.dependency, edit.old, edit.new);
                }
                if *dry_run { Ok(()) } else { apply_sync(&plan) }
            }
        };
        result
    }
}

fn discover_projects(root: &camino::Utf8Path) -> Result<Vec<Utf8PathBuf>, PolicyError> {
    let mut projects = vec![root.to_owned()];
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(directory.as_std_path()).map_err(|source| PolicyError::ReadConfig {
            path: directory.to_string(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| PolicyError::ReadConfig {
                path: directory.to_string(),
                source,
            })?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.')
                || name == "target"
                || (name == "fixtures" && directory.file_name() == Some("tests"))
            {
                continue;
            }
            let file_type = entry.file_type().map_err(|source| PolicyError::ReadConfig {
                path: entry.path().display().to_string(),
                source,
            })?;
            if !file_type.is_dir() {
                continue;
            }
            let path = Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| PolicyError::Sync {
                message: format!("nested crate path is not UTF-8: {}", path.display()),
            })?;
            if path.join("Cargo.toml").is_file() {
                projects.push(path.clone());
            }
            pending.push(path);
        }
    }
    projects[1..].sort();
    Ok(projects)
}
