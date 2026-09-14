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
use crate::load_baseline;
use crate::plan_sync;
use crate::render_inventory_json;
use crate::render_inventory_markdown;
use crate::render_json;
use crate::render_markdown;
use crate::scan_projects;

// qubit-style: allow multiple-public-types

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
    /// Policy operation.
    #[command(subcommand)]
    pub command: Command,
}

/// Supported policy operations.
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
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ReportFormat {
    /// Human-readable Markdown.
    Markdown,
    /// Machine-readable JSON.
    Json,
}

impl Cli {
    /// Loads the selected project configuration for the current command.
    pub fn load_config(&self) -> Result<ProjectConfig, PolicyError> {
        ProjectConfig::load(&self.project, self.config.as_deref())
    }

    /// Executes the selected command and writes its output to stdout.
    pub fn execute(&self) -> Result<(), PolicyError> {
        if let Command::Inventory {
            roots,
            format,
            output,
        } = &self.command
        {
            let inventory = scan_projects(roots)?;
            let text = match format {
                ReportFormat::Json => render_inventory_json(&inventory)?,
                ReportFormat::Markdown => render_inventory_markdown(&inventory),
            };
            if let Some(path) = output {
                std::fs::write(path.as_std_path(), format!("{text}\n")).map_err(|source| {
                    PolicyError::ReadConfig {
                        path: path.to_string(),
                        source,
                    }
                })?;
            } else {
                println!("{text}");
            }
            return Ok(());
        }
        let config = self.load_config()?;
        let baseline = load_baseline(&config, &self.project.join("target/policy-cache"))?;
        match &self.command {
            Command::Inventory { .. } => unreachable!("inventory handled before baseline loading"),
            Command::Check => {
                let evaluation = evaluate(&self.project, &config, &baseline)?;
                if evaluation.violations.is_empty() {
                    Ok(())
                } else {
                    Err(PolicyError::PolicyViolation {
                        count: evaluation.violations.len(),
                    })
                }
            }
            Command::Report { format } => {
                let evaluation = evaluate(&self.project, &config, &baseline)?;
                let report = Report::from_evaluation(evaluation, &baseline);
                let text = match format {
                    ReportFormat::Json => render_json(&report)?,
                    ReportFormat::Markdown => render_markdown(&report),
                };
                println!("{text}");
                Ok(())
            }
            Command::Sync { dry_run } => {
                let plan = plan_sync(&self.project, &baseline.baseline)?;
                for edit in &plan.manifest_edits {
                    println!("{}: {} -> {}", edit.dependency, edit.old, edit.new);
                }
                if *dry_run { Ok(()) } else { apply_sync(&plan) }
            }
        }
    }
}
