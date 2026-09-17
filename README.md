# rs-infra-dependency

[![Rust CI](https://github.com/qubit-ltd/rs-infra-dependency/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-infra-dependency/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-infra-dependency/coverage-badge.json)](https://qubit-ltd.github.io/rs-infra-dependency/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-infra-dependency.svg?color=blue)](https://crates.io/crates/qubit-infra-dependency)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

`rs-infra-dependency` gives an organization one small, readable baseline for
third-party Rust dependencies. It keeps direct declarations consistent and can
also require minimum versions for selected resolved packages such as security-
sensitive transitive dependencies, without replacing Cargo's resolver.

For example, a policy can require `num-bigint 0.4` across libraries, private
crates, and applications. Every project then declares `num-bigint = "0.4"`;
Cargo may adopt later `0.4.x` patches through its normal update process, but no
project can silently move to `0.5`.

## Installation

```bash
git clone https://github.com/qubit-ltd/rs-infra-dependency.git
cd rs-infra-dependency
cargo install --path .
```

This installs the generic Cargo subcommand `rs-infra-dependency`. Governed
repositories do not install the tool: local developers may install it once, and
GitHub Actions uses the reusable Action below.

## Create a baseline

Run the interactive generator in this repository:

```bash
./scripts/create-baseline.sh \
  --root /work/rust-common \
  --root /work/rust-platform \
  --internal-prefix acme- \
  --release v2026.09.13
```

Each `--root` is either a Rust project or a parent containing Rust projects one
level below. The generator ignores `path` and `workspace` dependencies, and
uses caller-supplied prefixes for published first-party crates. For each
conflicting external dependency, it asks once which Cargo requirement to use.
It writes an immediately usable, sorted file at
`policy/baselines/<release>.txt` (use a `.toml` output name for the new format):

```text
# package requirement
libc 0.2
num-bigint 0.4
serde 1.0
```

The legacy text format has no profile, exception, resolved-graph, or lockfile
rule. A non-comment line has exactly two fields: the package name and a Cargo
version requirement.

The TOML format separates direct declarations from selected resolved-package
minimums:

```toml
format = 3

[direct]
reqwest = "^0.13"
serde = "^1.0"

[resolved]
rustls = ">=0.23.45"
```

`[direct]` rules apply to `Cargo.toml`. `[resolved]` rules apply to every
matching package version recorded in the workspace `Cargo.lock`; a package
that is not present is allowed. Existing `.txt` baselines remain direct-only.

## Adopt and enforce it

Commit the baseline in a policy repository. Each governed repository keeps only
the following pointer at `.infra/dep/policy.toml`; it never copies the baseline:

```toml
format = 2
source = "https://github.com/example/rust-infra.git"
revision = "0123456789abcdef0123456789abcdef01234567"
baseline = "v2026.09.13"
internal-prefixes = ["acme-", "acme_"]
```

`revision` is the full immutable Git SHA containing the selected `.txt` or
`.toml` file.
The checker detached-checks-out that commit. `file://` sources work for local
development.

Check and synchronize a project:

```bash
rs-infra-dependency --project . check
rs-infra-dependency --project . sync --dry-run
rs-infra-dependency --project . sync
```

`check` rejects an external direct dependency missing from the baseline (`DP203`)
or declaring a different requirement (`DP202`). `sync` updates standard
`[dependencies]`, `[dev-dependencies]`, and `[build-dependencies]` entries while
preserving inline-table features; it does not change path/workspace dependencies.

When a TOML baseline has `[resolved]` rules, `check` validates the complete
locked graph with `cargo metadata --locked`. A checked-in `Cargo.lock` is never
changed by the checker. Library projects without a lock file use the reusable
Action's temporary lock-file mode; local checks can opt in explicitly:

```bash
rs-infra-dependency --project . check --temporary-lockfile
```

The temporary `Cargo.lock` is generated at the workspace root and removed after
the check. A resolved rule is a minimum version, for example `rustls >=0.23.45`;
all matching versions must satisfy it, while projects that do not use `rustls`
pass that rule.

Use the reusable Action in GitHub CI:

```yaml
- uses: qubit-ltd/rs-infra-dependency/.github/actions/check@<tool-commit-sha>
  with:
    project: .
    token: ${{ secrets.GITHUB_TOKEN }} # only for private policy sources
```

## Patch upgrades and limits

The direct baseline is a uniform declaration policy. `num-bigint 0.4` permits
Cargo's compatible `0.4.x` updates. Selected resolved rules protect minimum
versions in the lockfile without forcing every project to use exactly one
version. Applications
that commit `Cargo.lock` should run their usual upgrade verification after a
patch release:

```bash
cargo update
cargo test
```

Changing a minor or major line is deliberate: change the central baseline,
commit it, update each project pointer to that commit, run `sync`, and validate.
The tool does not automatically update transitive packages or replace Cargo's
resolver. It checks every matching lockfile version, including duplicates and
entries for other target platforms; `cargo audit` remains responsible for
advisories not yet represented in the baseline.

## Inventory

For a read-only multi-project inventory, use:

```bash
rs-infra-dependency inventory --root /work/rust-common --format markdown
```

The inventory is evidence for the generator; it is not a second policy format.

## Testing

```bash
# Run tests with the default feature set
cargo test

# Run tests with all declared features
cargo test --all-features

# Project CI checks
./ci-check.sh

# Check code coverage
./coverage.sh
```

## License

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the
full license text.

## Contributing

Contributions are welcome. Please follow the Rust API guidelines, keep public
API documentation and tests current, and run `./align-ci.sh` to format code and
`./ci-check.sh` to satisfy CI requirements before submitting a pull request.

## Author

**Haixing Hu** - *Qubit Co. Ltd.*

Repository: [https://github.com/qubit-ltd/rs-infra-dependency](https://github.com/qubit-ltd/rs-infra-dependency)
