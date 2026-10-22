#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
test_dir=$(mktemp -d)
trap 'rm -rf -- "$test_dir"' EXIT
output="${test_dir}/baseline.txt"
fixture="${test_dir}/fixture"
mkdir -p "$fixture/.infra/tools" "${test_dir}/bin"
cp "${repo_dir}/tests/fixtures/num-bigint-04/.infra/tools/defaults.toml" "$fixture/.infra/tools/defaults.toml"
cat >"${fixture}/Cargo.toml" <<'EOF'
[package]
name = "baseline-version-test"
version = "0.1.0"
edition = "2024"

[dependencies]
num-bigint = "^0.4.6"
clap = "2"
ort = "=2.0.0-rc.12"
syn = "^2.0.1"
quote = "=1.0.40"
EOF
printf '' >"${fixture}/src.rs"
cat >>"${fixture}/Cargo.toml" <<'EOF'

[lib]
path = "src.rs"
EOF
real_cargo=$(command -v cargo)
cat >"${test_dir}/bin/cargo" <<EOF
#!/usr/bin/env bash
if [[ "\$1" == info ]]; then
    [[ "\$2" == 'clap@2' ]] || exit 1
    printf 'clap\\nversion: 2.10.7 (latest 4.0.0)\\n'
else
    exec "$real_cargo" "\$@"
fi
EOF
chmod +x "${test_dir}/bin/cargo"

mkdir -p "${fixture}/fuzz"
cat >"${fixture}/fuzz/Cargo.toml" <<'EOF'
[package]
name = "baseline-nested-fuzz"
version = "0.1.0"
edition = "2024"

[workspace]

[lib]
path = "src.rs"

[dependencies]
serde_json = "^1.0"
EOF
printf '' >"${fixture}/fuzz/src.rs"

PATH="${test_dir}/bin:$PATH" "${repo_dir}/scripts/create-baseline.sh" \
  --root "$fixture" \
  --release vtest \
  --output "$output"

rg -x 'num-bigint \^0\.4' "$output"
rg -x 'clap \^2\.10' "$output"
rg -x 'ort >=2\.0\.0-rc\.12,<2\.1' "$output"
rg -x 'syn \^2\.0' "$output"
rg -x 'serde_json \^1\.0' "$output"
rg -x 'quote \^1\.0' "$output"
if rg -F '[profiles.' "$output"; then
    printf 'baseline 不应包含 profile 表。\n' >&2
    exit 1
fi

toml_output="${test_dir}/baseline.toml"
PATH="${test_dir}/bin:$PATH" "${repo_dir}/scripts/create-baseline.sh" \
  --root "$fixture" \
  --release vtest \
  --output "$toml_output"
rg -F 'ort = ">=2.0.0-rc.12,<2.1"' "$toml_output"

conflict_fixture="${test_dir}/conflict-fixture"
mkdir -p "${conflict_fixture}/.infra/tools"
cp "${repo_dir}/tests/fixtures/num-bigint-04/.infra/tools/defaults.toml" "${conflict_fixture}/.infra/tools/defaults.toml"
cat >"${conflict_fixture}/Cargo.toml" <<'EOF'
[package]
name = "baseline-conflict-test"
version = "0.1.0"
edition = "2024"

[lib]
path = "src.rs"

[dependencies]
clap = "4"

[dev-dependencies]
clap = "^4.5"

[build-dependencies]
clap = "^4.4"
EOF
printf '' >"${conflict_fixture}/src.rs"
conflict_output="${test_dir}/conflict.txt"
PATH="${test_dir}/bin:$PATH" "${repo_dir}/scripts/create-baseline.sh" \
  --root "$conflict_fixture" \
  --release vtest \
  --output "$conflict_output"
rg -x 'clap \^4\.5' "$conflict_output"
