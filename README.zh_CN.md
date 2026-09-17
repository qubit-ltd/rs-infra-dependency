# rs-infra-dependency

[![Rust CI](https://github.com/qubit-ltd/rs-infra-dependency/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-infra-dependency/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-infra-dependency/coverage-badge.json)](https://qubit-ltd.github.io/rs-infra-dependency/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-infra-dependency.svg?color=blue)](https://crates.io/crates/qubit-infra-dependency)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

`rs-infra-dependency` 用一份小而清晰的基线，统一组织内 Rust 项目的第三方依赖。它既能统一 `Cargo.toml` 中的直接依赖声明，也能为 `rustls` 这类安全敏感的解析依赖设置最低版本，但不会替代 Cargo 的解析器。

例如，基线写入 `num-bigint 0.4` 后，库、私有 crate 和应用都声明
`num-bigint = "0.4"`。Cargo 仍可按正常流程采用后续 `0.4.x` patch，任何项目却不能自行悄悄升到 `0.5`。

## 安装

```bash
git clone https://github.com/qubit-ltd/rs-infra-dependency.git
cd rs-infra-dependency
cargo install --path .
```

安装后提供通用 Cargo 子命令 `rs-infra-dependency`。受治理仓库不需要各自安装：开发者本地安装一次即可，GitHub Actions 则使用下方的复用 Action。

## 创建基线

在本仓库执行交互式生成脚本：

```bash
./scripts/create-baseline.sh \
  --root /work/rust-common \
  --root /work/rust-platform \
  --internal-prefix acme- \
  --release v2026.09.13
```

每个 `--root` 可以是 Rust 项目，也可以是其父目录；父目录会扫描一级子目录。脚本自动排除 `path`、`workspace` 依赖；已发布但仍属内部生态的 crate 通过调用方传入的 `--internal-prefix` 识别。发现同一个外部依赖存在不同声明时，脚本只询问一次应选择哪个 Cargo 版本要求。

结果默认写为可用且排序稳定的 `policy/baselines/<release>.txt`；输出文件名使用 `.toml` 时生成新格式：

```text
# package requirement
libc 0.2
num-bigint 0.4
serde 1.0
```

旧文本格式除注释外每一行只有两个字段：包名和 Cargo 版本要求。也可以使用 TOML 格式区分直接声明与解析依赖最低版本：

```toml
format = 3

[direct]
reqwest = "^0.13"
serde = "^1.0"

[resolved]
rustls = ">=0.23.45"
```

`[direct]` 检查 `Cargo.toml`；`[resolved]` 检查 workspace `Cargo.lock` 中出现的每个匹配版本。项目没有使用该包时通过。旧 `.txt` 基线继续兼容，并且只包含 direct 规则。

## 接入与检查

将基线提交到策略仓库后，每个受治理仓库只保存下面的指针配置
`.infra/dep/policy.toml`，无需复制基线内容：

```toml
format = 2
source = "https://github.com/example/rust-infra.git"
revision = "0123456789abcdef0123456789abcdef01234567"
baseline = "v2026.09.13"
internal-prefixes = ["acme-", "acme_"]
```

`revision` 是包含该 `.txt` 或 `.toml` 基线文件的完整、不可变 Git SHA；检查器会 detached checkout 到该提交。本地调试也可以使用 `file://` source。

检查和同步单个项目：

```bash
rs-infra-dependency --project . check
rs-infra-dependency --project . sync --dry-run
rs-infra-dependency --project . sync
```

若外部直接依赖未登记到基线，`check` 返回 `DP203`；若版本要求不同，则返回 `DP202`。`sync` 可以同步标准 `[dependencies]`、`[dev-dependencies]` 与 `[build-dependencies]`，并保留 inline table 中的 feature；它不会改动 path/workspace 依赖。

当 TOML 基线包含 `[resolved]` 规则时，`check` 会使用
`cargo metadata --locked` 检查完整解析图。检查器不会修改已提交的
`Cargo.lock`。没有锁文件的库项目由 GitHub Action 自动生成临时锁文件；本地可以显式运行：

```bash
rs-infra-dependency --project . check --temporary-lockfile
```

临时 `Cargo.lock` 位于 workspace 根目录，检查结束后会删除。解析规则是最低版本要求，例如 `rustls >=0.23.45`；所有匹配版本都必须满足要求，未使用 `rustls` 的项目不受影响。

GitHub CI 中使用复用 Action：

```yaml
- uses: qubit-ltd/rs-infra-dependency/.github/actions/check@<工具提交SHA>
  with:
    project: .
    token: ${{ secrets.GITHUB_TOKEN }} # 仅私有策略仓库需要
```

## Patch 升级与边界

直接依赖基线管理统一声明。`num-bigint 0.4` 允许 Cargo 采用兼容的 `0.4.x` patch；选定的 resolved 规则只保护锁文件中的最低版本，不要求所有项目精确使用同一版本。提交 `Cargo.lock` 的应用在 patch 发布后，应沿用自己的升级验证流程：

```bash
cargo update
cargo test
```

升级 minor 或 major 必须有意进行：先改中心基线、提交该基线、让各项目指向新提交、执行 `sync`，再验证。工具不会自动升级传递依赖，也不会替代 Cargo 的解析器；它会逐一检查锁文件中的重复版本和其他目标平台条目。`cargo audit` 继续负责发现尚未登记到基线的安全公告。

## 依赖盘点

仅需只读盘点多个项目时，运行：

```bash
rs-infra-dependency inventory --root /work/rust-common --format markdown
```

盘点结果是交互生成基线的依据，不是另一种策略格式。

## 测试

```bash
# 使用默认 feature 集运行测试
cargo test

# 使用项目声明的全部 feature 运行测试
cargo test --all-features

# 运行项目 CI 检查
./ci-check.sh

# 检查代码覆盖率
./coverage.sh
```

## 许可证

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

本项目基于 Apache License 2.0 授权。完整许可证文本请参阅
[LICENSE](LICENSE)。

## 贡献

欢迎贡献。请遵循 Rust API 指南，及时更新公共 API 文档与测试，并在提交
Pull Request 前运行 `./align-ci.sh`格式化代码，运行`./ci-check.sh`对齐CI要求。

## 作者

**Haixing Hu** - *Qubit Co. Ltd.*

仓库地址：[https://github.com/qubit-ltd/rs-infra-dependency](https://github.com/qubit-ltd/rs-infra-dependency)
