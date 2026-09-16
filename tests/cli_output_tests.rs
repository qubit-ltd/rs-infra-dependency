// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0 (the "License");
//    you may not use this file except in compliance with the License.
//    You may obtain a copy of the License at
//
//        http://www.apache.org/licenses/LICENSE-2.0
//
//    Unless required by applicable law or agreed to in writing, software
//    distributed under the License is distributed on an "AS IS" BASIS,
//    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//    See the License for the specific language governing permissions and
//    limitations under the License.
// =============================================================================

use std::process::Command;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rs-infra-dependency"))
}

#[test]
fn test_cli_success_reports_completion() {
    let output = binary()
        .args(["inventory", "--root", "tests/fixtures/application-no-lock"])
        .output()
        .expect("run inventory command");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("rs-infra-dependency: inventory succeeded"));
}

#[test]
fn test_cli_failure_reports_failure() {
    let output = binary()
        .args(["--project", "tests/fixtures/application-no-lock", "check"])
        .output()
        .expect("run check command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("rs-infra-dependency: check failed"));
}
