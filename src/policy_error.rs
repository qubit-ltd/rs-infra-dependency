// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Errors returned by policy loading, evaluation, and synchronization.

/// Errors returned while loading or validating policy configuration.
///
/// # Examples
///
/// ```
/// use qubit_infra_dependency::PolicyError;
///
/// let error = PolicyError::Unsupported { message: "demo".into() };
/// assert_eq!(error.code(), "DP105");
/// ```
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    /// The project configuration file could not be read.
    #[error("failed to read policy configuration {path}: {source}")]
    ReadConfig {
        /// The path that could not be read.
        path: String,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The project configuration TOML is invalid.
    #[error("invalid policy configuration {path}: {source}")]
    ParseConfig {
        /// The path containing invalid TOML.
        path: String,
        /// The underlying TOML error.
        source: toml::de::Error,
    },
    /// A policy configuration value violates the configuration contract.
    #[error("[{code}] {message}")]
    InvalidConfig {
        /// Stable machine-readable diagnostic code.
        code: &'static str,
        /// Human-readable diagnostic detail.
        message: String,
    },
    /// The selected policy source cannot be loaded or verified.
    #[error("[DP101] {message}")]
    Source {
        /// Human-readable source failure detail.
        message: String,
    },
    /// The selected baseline release cannot be loaded.
    #[error("[DP102] {message}")]
    Baseline {
        /// Human-readable baseline failure detail.
        message: String,
    },
    /// The baseline contents violate the baseline schema.
    #[error("[DP103] {message}")]
    InvalidBaseline {
        /// Human-readable schema failure detail.
        message: String,
    },
    /// Cargo metadata or lockfile evaluation failed.
    #[error("[{code}] {message}")]
    Cargo {
        /// Stable diagnostic code.
        code: &'static str,
        /// Human-readable Cargo failure detail.
        message: String,
    },
    /// Report serialization failed.
    #[error("[DP104] {message}")]
    Report {
        /// Human-readable serialization failure detail.
        message: String,
    },
    /// The project contains policy violations.
    #[error("[DP200] {count} policy violation(s) found")]
    PolicyViolation {
        /// Number of violations.
        count: usize,
    },
    /// A command is not yet available.
    #[error("[DP105] {message}")]
    Unsupported {
        /// Human-readable unsupported-command detail.
        message: String,
    },
    /// Manifest synchronization failed.
    #[error("[DP301] {message}")]
    Sync {
        /// Human-readable synchronization failure detail.
        message: String,
    },
}

impl PolicyError {
    /// Returns the stable diagnostic code for this error.
    ///
    /// The code is suitable for machine-readable CI handling and remains
    /// independent of the human-readable error message.
    ///
    /// # Returns
    ///
    /// Returns the stable diagnostic code assigned to this error variant.
    #[must_use]
    #[inline]
    pub fn code(&self) -> &'static str {
        match self {
            Self::ReadConfig { .. } | Self::ParseConfig { .. } => "DP001",
            Self::InvalidConfig { code, .. } => code,
            Self::Source { .. } => "DP101",
            Self::Baseline { .. } => "DP102",
            Self::InvalidBaseline { .. } => "DP103",
            Self::Cargo { code, .. } => code,
            Self::Report { .. } => "DP104",
            Self::PolicyViolation { .. } => "DP200",
            Self::Unsupported { .. } => "DP105",
            Self::Sync { .. } => "DP301",
        }
    }
}
