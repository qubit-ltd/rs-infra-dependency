// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Results of dependency synchronization planning.

use crate::FileEdit;
use crate::LockUpdate;
use crate::Violation;

/// Safe edits and changes blocked for manual review.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::SyncPlan;
///
/// let plan = SyncPlan {
///     manifest_edits: Vec::new(),
///     lock_updates: Vec::new(),
///     blocked: Vec::new(),
/// };
/// assert!(plan.manifest_edits.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPlan {
    /// Mechanical manifest edits that can be applied safely.
    pub manifest_edits: Vec<FileEdit>,
    /// Requested lock updates retained for callers that display them.
    pub lock_updates: Vec<LockUpdate>,
    /// Changes that must not be automated.
    pub blocked: Vec<Violation>,
}
