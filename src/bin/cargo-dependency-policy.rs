// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Compatibility entry point for the former `cargo dependency-policy` command.

use std::env;

use qubit_infra_dependency::run_cli;

fn main() {
    run_cli(env::args_os().collect());
}
