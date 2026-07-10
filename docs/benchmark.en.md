# Benchmarks

[Chinese](benchmark.md)

qweave is designed for large-panel factor research: compute a complete factor
set from a Polars DataFrame and return its result without splitting research
code into Python loops, temporary NumPy buffers, or compiled-artifact
management. This page publishes only rerun Windows measurements and records the
machine, commit, and commands.

## Takeaways

- **Against Qlib Alpha158DL:** for the complete 158-factor Alpha158 workload,
  qweave is **23.85× faster** by best time and uses about **46% less** peak RSS.
- **Against KunQuant Alpha101:** for KunQuant's supported 82-factor
  WorldQuant101 subset, with f64, low-precision fast statistics disabled, and
  input/output conversion plus JIT included, qweave is **2.56× faster** end to
  end and uses about **31% less** peak RSS.
- **Workflow:** qweave takes and returns Polars DataFrames. KunQuant converts the
  panel to contiguous `[time, stocks]` NumPy arrays, compiles a C++ bundle, then
  converts its output-array dictionary back to a Polars DataFrame; all of those
  steps are timed. Qlib Alpha158DL already returns a pandas DataFrame, but uses
  a local binary provider and handler workflow.
- **Cross-sectional factors:** `worldquant_alpha101()` places cross-sectional and
  time-series operators in one expression DAG and runs them on the Polars panel
  with one `compute_alphas` call. Compared with the provider/handler-oriented
  Alpha158 path, research code does not need to prepare a provider, load a
  handler, and then separately organize this computation.

Qlib and KunQuant ship different built-in factor sets, so their absolute times
are not ranked against each other: the Qlib comparison is Alpha158 only, while
the KunQuant comparison is its supported 82-factor Alpha101 subset only.

## Environment and Method

- Windows 11 Pro 10.0.26200; AMD Ryzen 9 9950X, 16 cores / 32 logical
  processors; 61.7 GiB RAM; Python 3.12.13; Rust 1.99.0-nightly release
  extension.
- Deterministic synthetic OHLCV panel: 6,000 symbols × 800 days = 4,800,000
  rows. One warmup and three measured runs per engine; best, mean, stdev, and
  process peak RSS are reported.
- `POLARS_MAX_THREADS=32` and `RAYON_NUM_THREADS=32`; Qlib and KunQuant both
  receive `--threads 32`. KunQuant runs in an x64 MSVC environment.
- Alpha158 was measured on 2026-07-10 at commit
  [`eb8c5d5`](https://github.com/GaomingOrion/qweave/commit/eb8c5d5);
  WorldQuant101 was measured on 2026-07-11 at commit
  [`ecbe2d7`](https://github.com/GaomingOrion/qweave/commit/ecbe2d7).

## qweave vs Qlib: All 158 Alpha158 Factors

Qlib `Alpha158DL` and qweave `qlib_alpha158()` both produce the complete
Alpha158 output. Qlib uses a local binary provider generated from the same
synthetic panel. Its loader returns a pandas DataFrame directly; this benchmark
does not apply a further output conversion.

| Engine | Best | Mean ± stdev | Factor cells/s | Process peak RSS |
| --- | ---: | ---: | ---: | ---: |
| qweave | **2.2379 s** | 2.3092 ± 0.0619 s | 338,887,623 | 10,324.1 MiB |
| Qlib Alpha158DL | 53.3692 s | 54.2714 ± 0.7821 s | 14,210,453 | 19,054.9 MiB |

By best time, qweave is **23.85× faster** and uses about 54% of Qlib's peak
memory. This describes the measured Alpha158 provider/handler path, not every
feature or workload of the wider Qlib platform.

## qweave vs KunQuant: 82-Factor WorldQuant101 Subset

Both paths compute the same 82 Alpha101 factors that KunQuant supports.
KunQuant is compiled as f64 (`double`), has `fast_stat` disabled, and receives
f64 input. Its end-to-end time includes sorting the Polars panel, converting it
to TS NumPy arrays, JIT compilation, `runGraph`, and converting the output-array
dictionary to a 4,800,000-row × 84-column Polars DataFrame.
`output_convert_s` reports the best output-conversion time.

| Engine | Best | Mean ± stdev | Factor cells/s | Process peak RSS | Best compile | Best output conversion |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| qweave | **3.1106 s** | 3.2749 ± 0.1449 s | 126,535,428 | 9,921.3 MiB | — | — |
| KunQuant f64 | 7.9515 s | 8.0905 ± 0.2087 s | 49,500,299 | 14,324.0 MiB | 3.0587 s | 0.6430 s |

End to end, qweave is **2.56× faster**. KunQuant's 3.0587 s compilation and
0.6430 s output-to-DataFrame conversion are both real parts of this research
workflow. Even after subtracting compilation, the remainder still mixes input
conversion, execution, and output conversion, so it is not a pure-compute
benchmark. qweave needs neither a C++ toolchain, JIT wait, or compiled-artifact
management, nor explicit DataFrame-to-NumPy-buffer round trips.

## Why qweave Performs Well

- **Rust and a shared DAG:** sorting, rolling windows, cross-sectional
  operators, and the DAG evaluator run in Rust. Shared subexpressions are
  reused across factors rather than recomputed factor by factor.
- **Managed intermediate lifetimes:** the evaluator reuses intermediate slots to
  reduce allocation; peak RSS is lower than the compared engine in both runs.
- **DataFrame-native interface:** Polars DataFrames are both the input and
  output, fitting directly into cleaning, labelling, evaluation, and downstream
  research workflows.
- **One expression path for time series and cross sections:** WorldQuant101
  time-series and cross-sectional factors can be batched together instead of
  being split into separate provider/handler stages.

These results apply to this version, machine, and synthetic data—not to all
hardware, versions, or factor sets. Re-run the commands below for the target
environment.

## Reproduce on Your Machine

```powershell
uv sync --dev
uv run maturin develop --uv --release
New-Item -ItemType Directory -Force C:\qweave-bench-tmp | Out-Null
$env:TMP = "C:\qweave-bench-tmp"
$env:TEMP = "C:\qweave-bench-tmp"
$env:POLARS_MAX_THREADS = "32"
$env:RAYON_NUM_THREADS = "32"
```

```powershell
uv run --frozen --with pyqlib python scripts\bench_factor_engines.py --workload alpha158 --engines qweave,qlib --symbols 6000 --days 800 --repeats 3 --warmups 1 --threads 32 --json results-alpha158.json
```

Run KunQuant from an x64 Developer PowerShell:

```powershell
uv run --frozen --with KunQuant --with setuptools python scripts\bench_factor_engines.py --workload worldquant101 --engines qweave,kunquant --symbols 6000 --days 800 --repeats 3 --warmups 1 --threads 32 --json results-worldquant101.json
```

`--symbols`, `--days`, `--repeats`, `--warmups`, `--names`, and `--threads`
control benchmark scale and parallelism. JSON records `compile_seconds` and
`output_conversion_seconds` when they apply to KunQuant. The script is
[scripts/bench_factor_engines.py](../scripts/bench_factor_engines.py).
