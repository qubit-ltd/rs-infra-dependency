#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_dir=$(cd -- "${script_dir}/.." && pwd)
output="${repo_dir}/target/dependency-inventory.json"
roots=()

usage() {
    cat <<'EOF'
Usage: bootstrap-baseline.sh --root <project-dir> [--root <project-dir> ...] [--output <file>]

Generates a candidate inventory only; does not modify project repositories or official baselines.
EOF
}

while (($# > 0)); do
    case "$1" in
        --root)
            (($# >= 2)) || { usage >&2; exit 2; }
            roots+=("$2")
            shift 2
            ;;
        --output)
            (($# >= 2)) || { usage >&2; exit 2; }
            output="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf 'Unknown argument: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

((${#roots[@]} > 0)) || {
    printf 'At least one --root is required; the script will not guess the organization repository scope.\n' >&2
    usage >&2
    exit 2
}

expanded=()
for root in "${roots[@]}"; do
    if [[ -f "${root}/Cargo.toml" ]]; then
        expanded+=("$root")
        continue
    fi
    found=0
    for manifest in "$root"/*/Cargo.toml; do
        [[ -f "$manifest" ]] || continue
        expanded+=("${manifest%/Cargo.toml}")
        found=1
    done
    ((found)) || {
        printf 'Not a Rust project directory or its immediate parent: %s\n' "$root" >&2
        exit 2
    }
done
roots=("${expanded[@]}")

valid_roots=()
for root in "${roots[@]}"; do
    if cargo metadata --manifest-path "${root}/Cargo.toml" --format-version 1 >/dev/null 2>&1; then
        valid_roots+=("$root")
    else
        printf 'Skipping project that could not be resolved: %s (fix Cargo version/path dependencies separately)\n' "$root" >&2
    fi
done
roots=("${valid_roots[@]}")
((${#roots[@]} > 0)) || { printf 'No Rust projects to scan.\n' >&2; exit 2; }

mkdir -p "$(dirname -- "$output")"
args=(run --quiet --bin rs-infra-dependency -- inventory)
for root in "${roots[@]}"; do
    args+=(--root "$root")
done
args+=(--format json --output "$output")

(cd "$repo_dir" && cargo "${args[@]}")
printf 'Candidate inventory generated: %s\n' "$output"
printf 'Review conflicts and version selections before converting the result to an official baseline with a revision.\n'
