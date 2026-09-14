#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)

cargo build --quiet --manifest-path "${repo_dir}/Cargo.toml" --bin rs-infra-dependency --bin cargo-dependency-policy
PATH="${repo_dir}/target/debug:${PATH}" rs-infra-dependency --help \
  | rg -F 'Usage: rs-infra-dependency'
PATH="${repo_dir}/target/debug:${PATH}" cargo dependency-policy --help \
  | rg -F 'Usage: cargo-dependency-policy'
