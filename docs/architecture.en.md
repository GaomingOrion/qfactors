# Architecture

[Chinese](architecture.md)

qweave is organized as a Rust workspace with a Python extension module.

## Crates

- `qweave-core`: panel layout, column validation, alpha expression evaluation,
  and result sinks.
- `qweave-factors`: built-in alpha builders for WorldQuant 101 and Qlib
  Alpha158.
- `qweave-eval`: forward-return labels, factor evaluation, correlation, report
  tables, and HTML output.
- `qweave-server`: Axum server for the interactive evaluation report.
- `qweave-py`: PyO3 extension module exposed to Python as `qweave`.

## Data Flow

1. Python or Rust callers provide a Polars DataFrame, symbol/time column names,
   and aliased alpha expressions.
2. `qweave-core` validates structural columns, sorts by `(symbol, time)`, and
   builds the internal cell set.
3. The evaluator computes expressions over the full panel.
4. Results are returned in memory, appended to the input in original row order
   through `with_alphas`, or written to Parquet through the sink layer.

## Alpha Evaluation

The DAG evaluator is the default alpha engine. It lowers requested expressions
into a shared DAG for common-subexpression reuse, slot reuse, and fused
elementwise chains. Set `QWEAVE_ENGINE=tree` to use the tree evaluator as an
independent reference.

```powershell
$env:QWEAVE_ENGINE = "tree"
uv run python -m pytest
Remove-Item Env:\QWEAVE_ENGINE
```
