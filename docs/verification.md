# Verification

验证日期：2026-09-13

工具链：rustc 1.94.0，cargo 1.94.0。

## 已验证

- 配置加载：默认 .infra/dependency/ 路径、缺失文件、非法 revision 和 profile。
- 基线加载：本地 policy source、release 选择和 schema 校验。
- 规则：直接依赖版本漂移、application 缺少 Cargo.lock、禁止 num-bigint 0.5.x。
- 报告：JSON 与 Markdown。
- 同步：plain-string 依赖版本的 dry-run，以及 inline-table 变更阻塞。
- 质量门禁：cargo fmt、cargo clippy --all-targets --all-features -- -D warnings、cargo test --all-targets --all-features、cargo test --doc、ci-check.sh。

## 当前边界

本阶段尚未接入现有 rs-ci，尚未迁移任何业务仓库，尚未实际回退 rs-budget，也尚未实现远程 Git source 的网络抓取。num-bigint 规则已能在 fixture 中发现 0.5.x，并生成受限版本同步计划；涉及别名、桥接代码和公开 API 的变更必须由专门 PR 完成。
