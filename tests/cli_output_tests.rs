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

use tempfile::NamedTempFile;

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

#[test]
fn test_cli_generates_and_removes_a_temporary_lockfile() {
    let project = std::path::Path::new("tests/fixtures/application-no-lock");
    let source = project
        .canonicalize()
        .expect("fixture project")
        .join("../policy-repo")
        .canonicalize()
        .expect("fixture policy repository");
    let config = NamedTempFile::new().expect("temporary policy config");
    let config_text = format!(
        "format = 2\nsource = \"file://{}\"\nrevision = \"0123456789abcdef0123456789abcdef01234567\"\nbaseline = \"v2026.09.2\"\n",
        source.display()
    );
    std::fs::write(config.path(), config_text).expect("policy config");
    let lockfile = project.join("Cargo.lock");
    assert!(!lockfile.exists());
    let output = binary()
        .args([
            "--project",
            project.to_str().expect("project path"),
            "--config",
            config.path().to_str().expect("config path"),
            "check",
            "--temporary-lockfile",
        ])
        .output()
        .expect("run temporary-lockfile check");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(!lockfile.exists());
}
