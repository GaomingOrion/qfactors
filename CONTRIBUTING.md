# 贡献指南

[English](CONTRIBUTING.en.md)

感谢你愿意改进 qweave。请保持改动聚焦、可验证。

## 环境

```powershell
uv sync --dev
uv run maturin develop
```

Rust 使用 `rust-toolchain.toml` 固定的 nightly toolchain。

## 必跑检查

提交 pull request 前运行：

```powershell
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run maturin develop
uv run python -m pytest
```

## 测试数据

不要提交私有市场数据、凭据、token、本地路径或生成的构建产物。仓库中
`crates/qweave-factors/tests` 下的 golden fixture 是合成测试数据。

如果有意的 alpha 实现变更需要更新 golden fixture，请运行测试失败信息中记录的
bless 流程，仔细 review diff，并在提交或 PR 中说明输出变化原因。

## 风格

- 匹配周围 Rust 和 Python 代码风格。
- API 变更必须同步到文档和测试。
- 优先提交小而可 review 的改动，避免宽泛重构。
- 行为变化需要添加聚焦测试。
