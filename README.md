# qfactors

qfactors is a Rust factor computation engine with Python bindings for Polars
panels. It provides:

- a Rust core for sorting, sampling, and computing panel factors;
- procedural macros for registering factor kernels;
- built-in factor and WorldQuant 101 alpha definitions;
- a Python extension module that accepts and returns Polars DataFrames.

The project is early-stage. APIs are usable for experimentation and internal
research workflows, but should be treated as pre-1.0.

## Roadmap

qfactors is pre-1.0 and under active development. The current focus is the
performance of the alpha expression engine while keeping results numerically
stable — a frozen golden baseline guards every change at `1e-8` tolerance.

**Done**

- v0.1.0 baseline frozen behind a golden regression safety net.
- `O(n)` rolling-window kernels (Welford variance, monotonic-deque min/max,
  rolling sum/mean/decay) replacing per-window recomputation.
- Global allocator (jemalloc on Unix, mimalloc on Windows).
- WorldQuant 101 alphas (`alpha1`–`alpha101`).

**In progress (0.2.x)**

- Experimental DAG evaluator (`QF_ENGINE=dag`) with hash-consed common
  subexpression elimination and slot-reuse. It is gated behind a flag and
  benchmarked against the default tree engine; an optimization is promoted only
  when it demonstrably beats the current default.

**Planned**

- Node-level parallelism and fewer layout transposes in the evaluator.
- Publish to PyPI and crates.io.
- Expanded factor / alpha catalog and API documentation.

## Installation

This repository currently targets source builds. It is not published to PyPI or
crates.io yet.

Prerequisites:

- Python 3.10 or newer
- `uv`
- Rust nightly with `rustfmt` and `clippy`

Set up a local development environment:

```bash
uv sync --dev
uv run maturin develop
```

The repository includes `rust-toolchain.toml`, so Cargo will use the pinned
nightly toolchain automatically.

## Quick Start

```python
import polars as pl
import qfactors

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

catalog = qfactors.alpha_catalog()
out = qfactors.compute_alphas(
    df=df,
    symbol_col="asset",
    time_col="time",
    alphas=["alpha101"],
    observation_times=[2],
)
```

`compute_panel` computes registered factor kernels. `compute_alphas` computes
registered alpha expressions. Both functions return a Polars DataFrame by
default, or a summary dict when `output_path` is provided.

## Public API

Python functions:

- `qfactors.compute_panel(df, symbol_col, time_col, factors, observation_times, column_aliases=None, output_path=None)`
- `qfactors.compute_alphas(df, symbol_col, time_col, alphas, observation_times, column_aliases=None, output_path=None)`
- `qfactors.factor_catalog()`
- `qfactors.alpha_catalog()`

Input rules:

- `symbol_col`, `time_col`, and `observation_times` cannot contain nulls.
- Structural NaN values are rejected.
- Float input nulls are converted to NaN so factor logic can propagate missing
  data.
- The engine sorts panel rows by `(symbol_col, time_col)` and rejects duplicate
  symbol-time pairs.
- `column_aliases` maps logical names such as `close` to physical input columns
  such as `adj_close`.

## Alpha Engine

`compute_alphas` uses the tree evaluator by default. An experimental DAG
evaluator can be selected for local benchmarking:

```bash
QF_ENGINE=dag uv run pytest
```

Valid values are `tree` and `dag`; invalid values raise an error. The tree
engine remains the default until the DAG path is fully benchmarked and promoted.

## WorldQuant 101

The built-in alpha catalog includes `alpha1` through `alpha101`. See
[docs/worldquant101.md](docs/worldquant101.md) for supported input fields,
coverage tiers, and implementation defaults.

This project is not affiliated with WorldQuant.

## Development Checks

Run the same checks expected by CI:

```bash
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run maturin develop
uv run pytest
```

See [docs/development.md](docs/development.md) for more detail.

## License

MIT. See [LICENSE](LICENSE).
