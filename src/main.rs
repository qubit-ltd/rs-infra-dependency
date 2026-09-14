// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Native entry point for rs-infra-dependency.

use std::env;

use qubit_infra_dependency::run_cli;

/// Parses command-line arguments and executes the selected policy operation.
fn main() {
    run_cli(env::args_os().collect());
}
