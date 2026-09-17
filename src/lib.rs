// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Public library API for checking and synchronizing direct Rust dependencies.
//!
//! The crate loads a project policy pointer, resolves its pinned baseline, and
//! exposes evaluation, reporting, inventory, and manifest-synchronization APIs.

pub mod baseline;
pub mod cargo;
pub mod cli;
mod config;
pub mod diagnostic;
pub mod inventory;
mod policy_error;
pub mod report;
pub mod rules;
pub mod source;
pub mod sync;
mod temporary_lockfile;

pub use baseline::Baseline;
pub use baseline::DependencyRequirement;
pub use cargo::ResolvedPackage;
pub use cli::run_cli;
pub use config::ProjectConfig;
pub use diagnostic::Violation;
pub use inventory::Inventory;
pub use inventory::InventoryDependency;
pub use inventory::InventoryProject;
pub use inventory::render_inventory_json;
pub use inventory::render_inventory_markdown;
pub use inventory::scan_projects;
pub use policy_error::PolicyError;
pub use report::BaselineIdentity;
pub use report::Report;
pub use report::render_json;
pub use report::render_markdown;
pub use rules::Evaluation;
pub use rules::evaluate;
pub use source::LoadedBaseline;
pub use source::load_baseline;
pub use sync::FileEdit;
pub use sync::LockUpdate;
pub use sync::SyncPlan;
pub use sync::apply_sync;
pub use sync::plan_sync;
