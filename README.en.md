# qweave

[Chinese](README.md)

qweave is a Rust-powered quantitative research workflow toolkit with Python
bindings for Polars panels. The current implementation covers factor and alpha
computation, forward-return labels, factor evaluation, and interactive reports.
The broader direction is an end-to-end workflow for factor research,
quantitative modeling, strategy construction, and backtesting.

The project is pre-1.0. The APIs are usable for research and internal workflows,
but they may still change.

## Why qweave

- **Polars-native workflow:** pass in a Polars DataFrame and get a Polars
  DataFrame back. `with_alphas` appends results in original row order;
  `compute_alphas` emits a full `(time, symbol)` panel.
- **Rust execution core:** panel sorting, validation, rolling windows,
  cross-sectional operators, and expression evaluation run in Rust.
- **Expression API:** compose alphas with `qweave.col("close")`,
  `qweave.lit(1.0)`, operators, windows, ranks, neutralization, and
  `replace_inputs()` templates.
- **Built-in factor libraries:** WorldQuant 101 and Qlib Alpha158 are exposed as
  expression builders with documented defaults and input aliasing.
- **Regression guarded:** built-in alphas are checked against frozen synthetic
  golden fixtures.
- **Factor evaluation:** labels, IC / RankIC, quantile returns, turnover,
  long-short diagnostics, factor correlation, HTML reports, and interactive
  reports are available. This surface is still experimental.

## Roadmap

**Done**

- WorldQuant 101 and Qlib Alpha158 expression libraries.
- Python expression API: `PyExpr`, `with_alphas`, `compute_alphas`, input
  replacement, and type stubs.
- DAG alpha evaluator as the default engine.
- Initial factor evaluation workflow and reports.

**Planned**

- Quantitative modeling, strategy construction, and backtesting modules.
- More complete factor and alpha API documentation.
- Publication to PyPI and crates.io.

## Installation

This repository currently targets source builds and is not published to PyPI or
crates.io yet.

Prerequisites:

- Python 3.10 or newer
- `uv`
- Rust nightly with `rustfmt` and `clippy`

```powershell
uv sync --dev
uv run maturin develop
```

The repository includes `rust-toolchain.toml`, so Cargo uses the pinned nightly
toolchain automatically.

## Quick Start

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

## Public API

- `qweave.compute_alphas(df, symbol_col, time_col, alphas, output_path=None)`
- `qweave.with_alphas(df, symbol_col, time_col, alphas)`
- `qweave.col(name)`, `qweave.lit(value)`, and expression operators
- `qweave.worldquant_alpha101(input_alias, alphas=None)`
- `qweave.qlib_alpha158(input_alias, alphas=None)`
- `qweave.with_labels(...)`, `qweave.evaluate(...)`
- `qweave.factor_correlation(...)`, `EvalResult.to_html(...)`,
  `EvalResult.view()`

## Alpha Engine

`compute_alphas` uses the DAG evaluator by default. The tree evaluator can be
selected as an independent reference implementation:

```powershell
$env:QWEAVE_ENGINE = "tree"
uv run python -m pytest
Remove-Item Env:\QWEAVE_ENGINE
```

## Documentation

- [Architecture](docs/architecture.en.md)
- [Development](docs/development.en.md)
- [Python Expression API](docs/expression_api.en.md)
- [Factor Evaluation](docs/factor_evaluation.en.md)
- [WorldQuant 101](docs/worldquant_alpha101.en.md)
- [Qlib Alpha158](docs/qlib_alpha158.en.md)
- [Benchmarks](docs/benchmark.en.md)

## Development Checks

```powershell
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run maturin develop
uv run python -m pytest
```

## License

MIT. See [LICENSE](LICENSE).
