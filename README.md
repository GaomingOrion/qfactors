# qweave

[English](README.en.md)

qweave 是一个 Rust 驱动的量化研究工作流工具包，面向 Polars 面板数据提供
Python 绑定。当前实现覆盖因子/alpha 计算、forward-return 标签、因子评估和
交互式报告；项目方向是逐步扩展为覆盖因子研究、量化建模、策略构建和回测的
端到端工具链。

项目仍处于 pre-1.0 阶段。当前 API 适合研究实验和内部工作流，但后续仍可能
调整。

## 为什么是 qweave

- **Polars 原生工作流：** 传入 Polars DataFrame，返回 Polars DataFrame。
  `with_alphas` 会按原始行序追加因子列；`compute_alphas` 会输出完整的
  `(time, symbol)` 面板，便于后续扫描和落盘。
- **Rust 执行核心：** 面板排序、结构校验、滚动窗口、截面算子和表达式求值
  都在 Rust 中执行，并在已验证有效的位置使用 rayon 并行。
- **表达式 API：** 使用 `qweave.col("close")`、`qweave.lit(1.0)`、算术运算、
  rolling window、rank、neutralization 和 `replace_inputs()` 模板快速构造
  alpha。
- **内置因子库：** `worldquant_alpha101()` 返回 `alpha1` 到 `alpha101`；
  `qlib_alpha158()` 返回 Qlib Alpha158 特征集。两者都以表达式对象暴露，
  并支持输入字段别名。
- **回归保护：** 内置 alpha 会对照冻结的合成 golden fixture 校验输出，
  用于防止引擎变更带来非预期数值漂移。
- **因子评估：** `with_labels` 追加 forward-return 标签；`evaluate` 计算
  IC / RankIC、分位收益、换手、long-short 组合等指标；
  `factor_correlation` 衡量因子冗余。该评估 API 仍标记为 experimental。

## 路线图

**已完成**

- WorldQuant 101 和 Qlib Alpha158 表达式因子库。
- Python 表达式 API：`PyExpr`、`with_alphas`、`compute_alphas`、输入字段替换
  和类型桩。
- DAG alpha evaluator：公共子表达式复用、slot 复用、节点级并行和 fused
  elementwise chain，当前作为默认引擎。
- 因子评估基础能力：标签、IC/RankIC、分位收益、换手、long-short 组合、
  HTML 报告和交互式报告。

**计划中**

- 量化建模、策略构建和回测模块。
- 更完整的因子/alpha API 文档。
- 发布到 PyPI 和 crates.io。

## 安装

当前仓库面向源码构建，尚未发布到 PyPI 或 crates.io。

前置要求：

- Python 3.10 或更新版本
- `uv`
- Rust nightly，包含 `rustfmt` 和 `clippy`

本地开发环境：

```powershell
uv sync --dev
uv run maturin develop
```

仓库包含 `rust-toolchain.toml`，Cargo 会自动使用固定的 nightly toolchain。

## 快速开始

```python
import polars as pl
import qweave

df = pl.DataFrame(
    {
        "asset": ["A", "A", "B", "B"],
        "time": [1, 2, 1, 2],
        "open": [10.0, 11.0, 20.0, 19.0],
        "close": [11.0, 12.0, 19.0, 21.0],
        "high": [12.0, 13.0, 21.0, 22.0],
        "low": [9.0, 10.0, 18.0, 18.5],
        "volume": [100.0, 120.0, 80.0, 90.0],
    }
)

alphas = qweave.worldquant_alpha101({}, alphas=["alpha101"])
out = qweave.compute_alphas(
    df=df,
    symbol_col="asset",
    time_col="time",
    alphas=alphas,
)

df_with_alpha = qweave.with_alphas(
    df=df,
    symbol_col="asset",
    time_col="time",
    alphas=[
        (
            (qweave.col("close") - qweave.col("open"))
            / (qweave.col("high") - qweave.col("low") + qweave.lit(0.001))
        ).alias("intraday_return")
    ],
)
```

`compute_alphas` 会在完整面板上计算表达式 alpha，默认返回 Polars DataFrame；
指定 `output_path` 时会写出 Parquet 并返回摘要。`with_alphas` 会把表达式输出
按输入 DataFrame 原始行序追加回去。

## 公开 API

Python 函数：

- `qweave.compute_alphas(df, symbol_col, time_col, alphas, output_path=None)`
- `qweave.with_alphas(df, symbol_col, time_col, alphas)`
- `qweave.col(name)`、`qweave.lit(value)` 和表达式运算符
- `qweave.worldquant_alpha101(input_alias, alphas=None)`
- `qweave.qlib_alpha158(input_alias, alphas=None)`
- `qweave.with_labels(...)`、`qweave.evaluate(...)`
- `qweave.factor_correlation(...)`、`EvalResult.to_html(...)`、
  `EvalResult.view()`

输入规则：

- `symbol_col` 和 `time_col` 不能包含 null。
- 结构列不允许 NaN。
- 浮点输入列中的 null 会转成 NaN，让因子逻辑自然传播缺失值。
- 引擎会按 `(symbol_col, time_col)` 排序，并拒绝重复的 symbol-time。
- 字段映射存在于表达式树中：使用 `PyExpr.replace_inputs()`，或使用内置因子库
  的 `input_alias` 参数。

## Alpha 引擎

`compute_alphas` 默认使用 DAG evaluator。也可以显式选择 tree evaluator，作为
独立参考实现：

```powershell
$env:QWEAVE_ENGINE = "tree"
uv run python -m pytest
Remove-Item Env:\QWEAVE_ENGINE
```

有效值是 `dag` 和 `tree`；非法值会报错。

## 因子库

- **WorldQuant 101**：见 [WorldQuant 101 文档](docs/worldquant_alpha101.md)。
- **Qlib Alpha158**：见 [Qlib Alpha158 文档](docs/qlib_alpha158.md)。

本项目与 WorldQuant、Microsoft 或 Qlib 没有关联。

## 文档

- [架构](docs/architecture.md)
- [开发](docs/development.md)
- [Python 表达式 API](docs/expression_api.md)
- [因子评估](docs/factor_evaluation.md)
- [WorldQuant 101](docs/worldquant_alpha101.md)
- [Qlib Alpha158](docs/qlib_alpha158.md)
- [基准测试](docs/benchmark.md)

## 开发检查

提交前运行：

```powershell
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run maturin develop
uv run python -m pytest
```

更多细节见 [开发文档](docs/development.md)。

## License

MIT. See [LICENSE](LICENSE).
