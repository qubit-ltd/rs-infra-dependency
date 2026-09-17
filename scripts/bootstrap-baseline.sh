#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_dir=$(cd -- "${script_dir}/.." && pwd)
output="${repo_dir}/target/dependency-inventory.json"
roots=()

usage() {
    cat <<'EOF'
用法：bootstrap-baseline.sh --root <项目目录> [--root <项目目录> ...] [--output <文件>]

只生成候选 inventory，不会修改任何业务仓库或正式 baseline。
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
            printf '未知参数：%s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

((${#roots[@]} > 0)) || {
    printf '至少需要一个 --root；脚本不会猜测组织仓库范围。\n' >&2
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
        printf '不是 Rust 项目目录或一级父目录：%s\n' "$root" >&2
        exit 2
    }
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

mkdir -p "$(dirname -- "$output")"
args=(run --quiet --bin rs-infra-dependency -- inventory)
for root in "${roots[@]}"; do
    args+=(--root "$root")
done
args+=(--format json --output "$output")

(cd "$repo_dir" && cargo "${args[@]}")
printf '候选 inventory 已生成：%s\n' "$output"
printf '请审核 conflicts 和版本选择后，再把结果转为带 revision 的正式 baseline。\n'
