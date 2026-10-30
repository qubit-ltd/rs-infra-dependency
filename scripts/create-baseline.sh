#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
repo_dir=$(cd -- "${script_dir}/.." && pwd -P)
release="v$(date +%Y.%m.%d)"
output=""
roots=()
internal_prefixes=()

usage() {
    cat <<'EOF'
Usage: create-baseline.sh --root <project-dir> [--root <project-dir> ...] [options]

Options:
  --release <version>   Baseline release, defaults to the current date as vYYYY.MM.DD
  --output <file>       Output baseline file (.toml generates format=3)
  --internal-prefix <prefix>  Internal crate namespace prefix (repeatable; no registry crates excluded by default)
  Enter c to set a custom Cargo requirement when versions conflict
  --help                Show this help

The script prompts for each conflicting dependency and generates a baseline ready for integration. It does not modify project repositories.
EOF
}

while (($# > 0)); do
    case "$1" in
        --root) (($# >= 2)) || { usage >&2; exit 2; }; roots+=("$2"); shift 2 ;;
        --release) (($# >= 2)) || { usage >&2; exit 2; }; release="$2"; shift 2 ;;
        --output) (($# >= 2)) || { usage >&2; exit 2; }; output="$2"; shift 2 ;;
        --internal-prefix) (($# >= 2)) || { usage >&2; exit 2; }; internal_prefixes+=("$2"); shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'Unknown argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done

[[ "$release" =~ ^[[:alnum:]][[:alnum:]._-]*$ ]] || {
    printf 'Invalid baseline release: %s\n' "$release" >&2
    exit 2
}
default_output="${repo_dir}/conf/policy/baselines/${release}.txt"
[[ -n "$output" ]] || output="$default_output"
if [[ "$output" != /* ]]; then output="$(pwd -P)/$output"; fi

((${#roots[@]} > 0)) || { printf 'At least one --root is required.\n' >&2; usage >&2; exit 2; }
command -v jq >/dev/null || { printf 'jq is required to interactively select dependency versions.\n' >&2; exit 2; }

expanded=()
for root_index in "${!roots[@]}"; do
    root=${roots[$root_index]}
    printf '[1/5] Scanning project directory (%d/%d): %s\n' "$((root_index + 1))" "${#roots[@]}" "$root" >&2
    mapfile -d '' -t manifests < <(
        find "$root" -mindepth 1 -type d \( -name '.*' -o -name target -o -path '*/tests/fixtures' \) -prune -o \
            -type f -name Cargo.toml -print0 | sort -z
    )
    ((${#manifests[@]} > 0)) || { printf 'No Rust projects found: %s\n' "$root" >&2; exit 2; }
    for manifest in "${manifests[@]}"; do
        expanded+=("${manifest%/Cargo.toml}")
    done
done
roots=("${expanded[@]}")

valid_roots=()
for root_index in "${!roots[@]}"; do
    root=${roots[$root_index]}
    printf '[2/5] Resolving Cargo projects (%d/%d): %s\n' "$((root_index + 1))" "${#roots[@]}" "$root" >&2
    if cargo metadata --manifest-path "${root}/Cargo.toml" --format-version 1 >/dev/null 2>&1; then
        valid_roots+=("$root")
    else
        printf 'Skipping project that could not be resolved: %s (fix Cargo version/path dependencies separately)\n' "$root" >&2
    fi
done
roots=("${valid_roots[@]}")
((${#roots[@]} > 0)) || { printf 'No Rust projects to scan.\n' >&2; exit 2; }

inventory=$(mktemp)
rules=$(mktemp)
validation_root=$(mktemp -d)
current_tmp=""
manifest_tmp=""
cleanup() {
    rm -f -- "$inventory" "$rules"
    rm -rf -- "$validation_root"
    [[ -z "$current_tmp" ]] || rm -f -- "$current_tmp"
    [[ -z "$manifest_tmp" ]] || rm -f -- "$manifest_tmp"
}
trap cleanup EXIT

valid_requirement() {
    local crate="$1" requirement="$2"
    [[ -n "$requirement" && "$requirement" != *$'\n'* && "$requirement" != *$'\r'* && "$requirement" != *$'\t'* ]] || return 1
    local requirement_pattern='^[[:alnum:].,*+<=>^~_ -]+$'
    [[ "$requirement" =~ $requirement_pattern ]] || return 1
    : >"$validation_root/lib.rs"
    cat >"$validation_root/Cargo.toml" <<EOF
[package]
name = "baseline-requirement-validation"
version = "0.0.0"
edition = "2024"

[lib]
path = "lib.rs"

[dependencies]
${crate} = "${requirement}"
EOF
    cargo metadata --manifest-path "$validation_root/Cargo.toml" --format-version 1 --no-deps >/dev/null 2>&1
}

normalize_requirement() {
    local crate="$1" requirement="$2" major info
    if [[ "$requirement" =~ ^[=^]?([0-9]+)\.([0-9]+)\.([0-9]+)-([0-9A-Za-z.-]+)$ ]]; then
        printf '>=%s.%s.%s-%s,<%s.%s\n' \
            "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "${BASH_REMATCH[3]}" "${BASH_REMATCH[4]}" \
            "${BASH_REMATCH[1]}" "$((10#${BASH_REMATCH[2]} + 1))"
    elif [[ "$requirement" =~ ^[=^]?([0-9]+)\.([0-9]+)(\.[0-9]+)?$ ]]; then
        printf '^%s.%s\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}"
    elif [[ "$requirement" =~ ^\^?([0-9]+)$ ]]; then
        major="${BASH_REMATCH[1]}"
        if ! info=$(cargo info "${crate}@${major}" --quiet); then
            printf 'Could not query published %s.x versions of %s.\n' "$major" "$crate" >&2
            return 1
        fi
        if [[ "$info" =~ (^|$'\n')version:[[:space:]]([0-9]+)\.([0-9]+)\.[0-9]+([[:space:]]|$) ]] \
            && [[ "${BASH_REMATCH[2]}" == "$major" ]]; then
            printf '^%s.%s\n' "$major" "${BASH_REMATCH[3]}"
        else
            printf 'No stable published %s.x version found for %s.\n' "$major" "$crate" >&2
            return 1
        fi
    else
        printf '%s\n' "$requirement"
    fi
}

args=(run --quiet --bin rs-infra-dependency -- inventory)
for root in "${roots[@]}"; do args+=(--root "$root"); done
args+=(--format json --output "$inventory")
printf '[3/5] Collecting dependency inventories from %d Cargo projects; the first Cargo build may take some time.\n' "${#roots[@]}" >&2
(cd "$repo_dir" && cargo "${args[@]}")

mkdir -p "$(dirname -- "$output")"
dependency_count=$(jq '.direct_requirements | length' "$inventory")
printf '[4/5] Processing %d direct dependencies; you will be prompted to choose when versions conflict.\n' "$dependency_count" >&2
while IFS= read -r name; do
        internal=0
        for prefix in "${internal_prefixes[@]}"; do
            if [[ "$name" == "${prefix}"* ]]; then internal=1; break; fi
        done
        ((internal)) && continue
        mapfile -t choices < <(jq -r --arg n "$name" '.direct_requirements[$n][]' "$inventory")
        selected="${choices[0]}"
        if ((${#choices[@]} > 1)); then
            simple_stable=1
            highest_major=-1
            highest_minor=-1
            highest_patch=-1
            for choice in "${choices[@]}"; do
                if [[ ! "$choice" =~ ^[=^]?([0-9]+)(\.([0-9]+))?(\.([0-9]+))?$ ]]; then
                    simple_stable=0
                    break
                fi
                choice_major=$((10#${BASH_REMATCH[1]}))
                choice_minor=0
                choice_patch=0
                [[ -n "${BASH_REMATCH[3]:-}" ]] && choice_minor=$((10#${BASH_REMATCH[3]}))
                [[ -n "${BASH_REMATCH[5]:-}" ]] && choice_patch=$((10#${BASH_REMATCH[5]}))
                if ((choice_major > highest_major ||
                    (choice_major == highest_major && choice_minor > highest_minor) ||
                    (choice_major == highest_major && choice_minor == highest_minor && choice_patch > highest_patch))); then
                    highest_major=$choice_major
                    highest_minor=$choice_minor
                    highest_patch=$choice_patch
                    selected="$choice"
                fi
            done
        fi
        if ((${#choices[@]} > 1 && !simple_stable)); then
            printf '\nDependency %s has multiple declared versions:\n' "$name" >&2
            for i in "${!choices[@]}"; do printf '  %d) %s\n' "$((i + 1))" "${choices[$i]}" >&2; done
            printf '  c) Enter a custom Cargo requirement\n' >&2
            [[ -r /dev/tty ]] || { printf 'Cannot access an interactive terminal. Run this in a terminal to select a version.\n' >&2; exit 2; }
            while true; do
                if ! read -r -p "Select [1-${#choices[@]}] or c (default 1, q to quit): " answer </dev/tty; then
                    printf '\nNo interactive input detected. Run this in a terminal and select a version; no baseline was generated.\n' >&2
                    exit 2
                fi
                answer=${answer:-1}
                [[ "$answer" == q ]] && { printf 'Cancelled by user; no baseline written.\n' >&2; exit 1; }
                if [[ "$answer" == c ]]; then
                    while true; do
                        if ! read -r -p "Enter the Cargo version requirement for ${name}: " custom_requirement </dev/tty; then
                            printf '\nNo interactive input detected. Run this in a terminal and enter a version requirement; no baseline was generated.\n' >&2
                            exit 2
                        fi
                        if valid_requirement "$name" "$custom_requirement"; then
                            selected="$custom_requirement"
                            break
                        fi
                        printf 'Invalid Cargo version requirement. Try again (for example, ^1.2 or >=1.2, <2).\n' >&2
                    done
                    break
                fi
                [[ "$answer" =~ ^[0-9]+$ && "$answer" -ge 1 && "$answer" -le "${#choices[@]}" ]] && { selected="${choices[$((answer - 1))]}"; break; }
                printf 'Enter a valid number.\n' >&2
            done
        fi
        selected=$(normalize_requirement "$name" "$selected")
        printf '%s\t%s\n' "$name" "$selected" >>"$rules"
done < <(jq -r '.direct_requirements | keys[]' "$inventory")

printf '[5/5] Writing baseline file: %s\n' "$output" >&2
if [[ "$output" == *.toml ]]; then
    {
        printf 'format = 3\n\n[direct]\n'
        while IFS=$'\t' read -r name selected; do
            printf '%s = "%s"\n' "$name" "$selected"
        done <"$rules"
        printf '\n[resolved]\n'
    } >"$output"
else
    {
        printf '# package requirement\n'
        while IFS=$'\t' read -r name selected; do
            # The text format stores each requirement as one token. Cargo
            # accepts comparator lists without spaces after commas.
            selected=${selected//, /,}
            selected=${selected#\"}
            selected=${selected%\"}
            printf '%s %s\n' "$name" "$selected"
        done <"$rules"
    } >"$output"
fi

printf 'Baseline generated: %s\n' "$output"
baseline_dir="${repo_dir}/conf/policy/baselines"
manifest_source=""
if [[ "$output" == "${baseline_dir}/${release}.txt" ]]; then
    manifest_source="policy/baselines/${release}.txt"
elif [[ "$output" == "${baseline_dir}/${release}.toml" ]]; then
    manifest_source="policy/baselines/${release}.toml"
fi

if [[ -n "$manifest_source" ]]; then
    printf 'Updating the current baseline pointer and conf/manifest.json.\n' >&2
    current_path="${repo_dir}/conf/policy/current.toml"
    manifest_path="${repo_dir}/conf/manifest.json"
    [[ -f "$current_path" && -f "$manifest_path" ]] || {
        printf 'Error: current.toml or conf/manifest.json is missing; the baseline pointer was not updated.\n' >&2
        exit 1
    }
    current_tmp=$(mktemp "${current_path}.XXXXXX")
    manifest_tmp=$(mktemp "${manifest_path}.XXXXXX")
    printf 'baseline = "%s"\n' "$release" >"$current_tmp"
    jq --arg source "$manifest_source" --arg target "$manifest_source" \
        'if any(.files[]; .source == $source) then
            .files |= map(if .source == $source then .target = $target else . end)
         else
            .files += [{"source": $source, "target": $target}]
         end' \
        "$manifest_path" >"$manifest_tmp"
    mv -- "$current_tmp" "$current_path"
    current_tmp=""
    mv -- "$manifest_tmp" "$manifest_path"
    manifest_tmp=""
    printf 'Current baseline updated to %s and registered in conf/manifest.json.\n' "$release"
else
    printf 'Custom output path is outside the standard baseline directory; current.toml and conf/manifest.json were not updated.\n'
fi
