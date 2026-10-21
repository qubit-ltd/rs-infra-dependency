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
用法：create-baseline.sh --root <项目目录> [--root <项目目录> ...] [选项]

选项：
  --release <版本>   baseline release，默认当前日期 vYYYY.MM.DD
  --output <文件>    输出 baseline 文件（.toml 生成 format=3）
  --internal-prefix <前缀>  声明内部 crate 命名空间前缀（可重复；默认不排除任何 registry crate）
  版本冲突时输入 c 可自定义 Cargo 版本要求
  --help             显示帮助

脚本会逐项询问冲突依赖选择，生成可直接接入的 baseline，不修改业务仓库。
EOF
}

while (($# > 0)); do
    case "$1" in
        --root) (($# >= 2)) || { usage >&2; exit 2; }; roots+=("$2"); shift 2 ;;
        --release) (($# >= 2)) || { usage >&2; exit 2; }; release="$2"; shift 2 ;;
        --output) (($# >= 2)) || { usage >&2; exit 2; }; output="$2"; shift 2 ;;
        --internal-prefix) (($# >= 2)) || { usage >&2; exit 2; }; internal_prefixes+=("$2"); shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) printf '未知参数：%s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done

[[ "$release" =~ ^[[:alnum:]][[:alnum:]._-]*$ ]] || {
    printf '无效的 baseline release：%s\n' "$release" >&2
    exit 2
}
default_output="${repo_dir}/conf/policy/baselines/${release}.txt"
[[ -n "$output" ]] || output="$default_output"
if [[ "$output" != /* ]]; then output="$(pwd -P)/$output"; fi

((${#roots[@]} > 0)) || { printf '至少需要一个 --root。\n' >&2; usage >&2; exit 2; }
command -v jq >/dev/null || { printf '需要 jq 才能交互选择依赖版本。\n' >&2; exit 2; }

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
    ((found)) || { printf '不是 Rust 项目目录或一级父目录：%s\n' "$root" >&2; exit 2; }
done
roots=("${expanded[@]}")

valid_roots=()
for root in "${roots[@]}"; do
    if cargo metadata --manifest-path "${root}/Cargo.toml" --format-version 1 >/dev/null 2>&1; then
        valid_roots+=("$root")
    else
        printf '跳过无法解析的项目：%s（请单独修复 Cargo 版本/path 依赖）\n' "$root" >&2
    fi
done
roots=("${valid_roots[@]}")
((${#roots[@]} > 0)) || { printf '没有可扫描的 Rust 项目。\n' >&2; exit 2; }

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

args=(run --quiet --bin rs-infra-dependency -- inventory)
for root in "${roots[@]}"; do args+=(--root "$root"); done
args+=(--format json --output "$inventory")
(cd "$repo_dir" && cargo "${args[@]}")

mkdir -p "$(dirname -- "$output")"
while IFS= read -r name; do
        internal=0
        for prefix in "${internal_prefixes[@]}"; do
            if [[ "$name" == "${prefix}"* ]]; then internal=1; break; fi
        done
        ((internal)) && continue
        mapfile -t choices < <(jq -r --arg n "$name" '.direct_requirements[$n][]' "$inventory")
        selected="${choices[0]}"
        if ((${#choices[@]} > 1)); then
            printf '\n依赖 %s 存在多个声明版本：\n' "$name" >&2
            for i in "${!choices[@]}"; do printf '  %d) %s\n' "$((i + 1))" "${choices[$i]}" >&2; done
            printf '  c) 自定义 Cargo 版本要求\n' >&2
            [[ -r /dev/tty ]] || { printf '无法访问交互终端，请在终端运行后选择版本。\n' >&2; exit 2; }
            while true; do
                if ! read -r -p "选择 [1-${#choices[@]}] 或 c（默认 1，输入 q 放弃）：" answer </dev/tty; then
                    printf '\n未检测到交互输入；请在终端运行并选择版本，未生成 baseline。\n' >&2
                    exit 2
                fi
                answer=${answer:-1}
                [[ "$answer" == q ]] && { printf '用户取消，未写入 baseline。\n' >&2; exit 1; }
                if [[ "$answer" == c ]]; then
                    while true; do
                        if ! read -r -p "请输入 ${name} 的 Cargo 版本要求：" custom_requirement </dev/tty; then
                            printf '\n未检测到交互输入；请在终端运行并输入版本要求，未生成 baseline。\n' >&2
                            exit 2
                        fi
                        if valid_requirement "$name" "$custom_requirement"; then
                            selected="$custom_requirement"
                            break
                        fi
                        printf '无效的 Cargo 版本要求，请重新输入（例如 ^1.2 或 >=1.2, <2）。\n' >&2
                    done
                    break
                fi
                [[ "$answer" =~ ^[0-9]+$ && "$answer" -ge 1 && "$answer" -le "${#choices[@]}" ]] && { selected="${choices[$((answer - 1))]}"; break; }
                printf '请输入有效编号。\n' >&2
            done
        fi
        printf '%s\t%s\n' "$name" "$selected" >>"$rules"
done < <(jq -r '.direct_requirements | keys[]' "$inventory")

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

printf 'baseline 已生成：%s\n' "$output"
baseline_dir="${repo_dir}/conf/policy/baselines"
manifest_source=""
if [[ "$output" == "${baseline_dir}/${release}.txt" ]]; then
    manifest_source="policy/baselines/${release}.txt"
elif [[ "$output" == "${baseline_dir}/${release}.toml" ]]; then
    manifest_source="policy/baselines/${release}.toml"
fi

if [[ -n "$manifest_source" ]]; then
    current_path="${repo_dir}/conf/policy/current.toml"
    manifest_path="${repo_dir}/conf/manifest.json"
    [[ -f "$current_path" && -f "$manifest_path" ]] || {
        printf '错误：缺少 current.toml 或 conf/manifest.json，未更新基线指针。\n' >&2
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
    printf '已将当前基线更新为 %s，并登记到 conf/manifest.json。\n' "$release"
else
    printf '自定义输出路径不在标准 baseline 目录，未更新 current.toml 和 conf/manifest.json。\n'
fi
