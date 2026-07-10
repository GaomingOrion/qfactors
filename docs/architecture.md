# 架构

[English](architecture.en.md)

qweave 是一个 Rust workspace，并通过 PyO3 暴露 Python 扩展模块。

## Crates

- `qweave-core`：面板布局、列校验、alpha 表达式求值和结果 sink。
- `qweave-factors`：内置 alpha builder，包括 WorldQuant 101 和 Qlib Alpha158。
- `qweave-eval`：forward-return 标签、因子评估、相关性、报告表和 HTML 输出。
- `qweave-server`：交互式评估报告的 Axum 服务。
- `qweave-py`：PyO3 扩展模块，对 Python 暴露为 `qweave`。

## 数据流

1. Python 或 Rust 调用方提供 Polars DataFrame、symbol/time 列名和带 alias 的
   alpha 表达式列表。
2. `qweave-core` 校验结构列，按 `(symbol, time)` 排序，并构造内部 cell set。
3. evaluator 在完整面板上计算表达式。
4. 结果可以以内存 DataFrame 返回、通过 `with_alphas` 追加到原始行序的输入
   DataFrame，或通过 sink 层写出 Parquet。

## Alpha 求值

默认 alpha evaluator 是 DAG 引擎。它会把请求的表达式降成共享 DAG，用于公共子
表达式复用、slot 复用和 fused elementwise chain。可以通过
`QWEAVE_ENGINE=tree` 切到 tree evaluator，作为独立参考实现。

```powershell
$env:QWEAVE_ENGINE = "tree"
uv run python -m pytest
Remove-Item Env:\QWEAVE_ENGINE
```
