// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Native entry point for rs-infra-dependency.

/// Parses command-line arguments and executes the selected policy operation.
fn main() {
    qubit_infra_dependency::run_cli(std::env::args_os().collect());
}
