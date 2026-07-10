# Benchmarks

[Chinese](benchmark.md)

This document records the benchmark method and reproduction commands for
qweave. Because the project development environment has moved from macOS to
Windows, previous performance measurements are no longer a valid reference.
Any published performance claim should be re-measured in the current
Windows/PowerShell environment and recorded with the date, machine profile, and
exact command.

The benchmark script uses deterministic synthetic OHLCV panels, so runs are
repeatable without external market data.

## Scope

The current comparisons focus on representative factor-library execution paths:

- Qlib `Alpha158DL`, compared with qweave's `qlib_alpha158()` expression library.
- KunQuant Alpha101 JIT execution, compared with qweave's
  `worldquant_alpha101()` expression library.

## Environment Notes

- Commands are written for Windows PowerShell.
- Python dependencies are managed with `uv`.
- Build the qweave extension in release mode before measuring:

```powershell
uv run maturin develop --uv --release
```

Qlib and KunQuant's dependency/JIT paths are not robust to non-ASCII user-profile
or workspace paths. If your paths contain non-ASCII characters, point temporary
directories at an ASCII-only path:

```powershell
$env:TMP = "C:\qweave-bench-tmp"
$env:TEMP = "C:\qweave-bench-tmp"
```

Qlib can pay a one-time cold-start cost in a fresh `uv run --with pyqlib`
environment. Use at least `--warmups 1` when comparing computation time.

## Qlib Alpha158

```powershell
uv run --frozen --with pyqlib python scripts\bench_factor_engines.py --workload alpha158 --engines qweave,qlib --symbols 6000 --days 800 --repeats 3 --warmups 1 --threads 1
```

When recording results, include:

- Date and commit SHA.
- Windows version, CPU, and memory.
- `symbols`, `days`, `repeats`, `warmups`, and `threads`.
- qweave, Qlib, Python, and Rust toolchain versions.
- `best`, `mean`, `stdev`, rows/s, and cells/s.

## KunQuant WorldQuant101

```powershell
uv run --frozen --with KunQuant --with setuptools python scripts\bench_factor_engines.py --workload worldquant101 --engines qweave,kunquant --symbols 6000 --days 800 --repeats 3 --warmups 1 --threads 1
```

KunQuant timing includes JIT compilation because that is the end-to-end cost
users pay when compiling a fresh expression bundle. Keep `compile_s` in recorded
results so compile and run time can be interpreted separately.

## Useful Options

- `--symbols` and `--days` scale the synthetic panel.
- `--repeats` and `--warmups` control timing runs.
- `--names` selects a comma-separated factor subset.
- `--json results.json` saves machine-readable results.

The benchmark script is
[scripts/bench_factor_engines.py](../scripts/bench_factor_engines.py).
