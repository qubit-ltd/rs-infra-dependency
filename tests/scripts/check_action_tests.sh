#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
action="$repo_root/.github/actions/check/action.yml"

fail() {
  echo "check_action_tests: $*" >&2
  exit 1
}

[[ -f "$action" ]] || fail "missing $action"

grep -Fq 'project:' "$action" || fail "missing project input"
grep -Fq 'default: .' "$action" || fail "project must default to ."
grep -Fq 'token:' "$action" || fail "missing optional token input"
grep -Fq 'required: false' "$action" || fail "token must be optional"
grep -Fq '/../../..' "$action" || fail "action must install from repository root"
grep -Fq 'cargo install --path' "$action" || fail "action must install the CLI"
grep -Fq 'rs-infra-dependency' "$action" || fail "action must run the native command"
grep -Fq -- '--project' "$action" || fail "action must pass the project input"
grep -Fq 'check' "$action" || fail "action must run check"

echo "check_action_tests: PASS"
