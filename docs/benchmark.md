# Benchmarks

This document records cross-engine factor-computation benchmarks for qfactors.
The benchmark script uses deterministic synthetic OHLCV panels so runs are
repeatable without external market data.

## Scope

The published comparison focuses on engines with representative factor-library
execution paths:

- Qlib `Alpha158DL`, compared with qfactors' `qlib_alpha158()` expression
  library.
- KunQuant Alpha101 JIT execution, compared with qfactors'
  `worldquant_alpha101()` expression library.


## Environment Notes

Last measured: 2026-07-04.

- Host: Windows, PowerShell.
- Dataset size for the recorded run: 3000 symbols x 400 days, or 1,200,000
  rows.
- Python dependencies are managed with `uv`.
- qfactors extension was built with `uv run maturin develop --uv --release`.
- On Windows, ASCII-only cache and temporary paths are used because Qlib and
  KunQuant dependency/JIT paths are not robust to the non-ASCII user directory
  in this workspace.

Shared setup:

```powershell
$env:UV_CACHE_DIR = "C:\qfactors-uv-cache"
$env:UV_PYTHON_INSTALL_DIR = "C:\qfactors-uv-python"
$env:QFACTORS_BENCH_TMP = "C:\qfactors-bench-tmp"
$env:TMP = $env:QFACTORS_BENCH_TMP
$env:TEMP = $env:QFACTORS_BENCH_TMP
uv run maturin develop --uv --release
```

## Qlib Alpha158

Command:

```powershell
uv run --frozen --with pyqlib python scripts\bench_factor_engines.py --workload alpha158-lite --engines qfactors,qlib --symbols 3000 --days 400 --repeats 3 --warmups 1 --threads 1
```

Results:

| engine | workload | rows | factors | best_s | mean_s | cells/s |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| qfactors | alpha158-lite | 1,200,000 | 30 | 0.7864 | 1.0535 | 45,779,103 |
| qlib | alpha158-lite | 1,200,000 | 30 | 63.8869 | 64.5340 | 563,495 |

Interpretation: qfactors is about 81x faster than Qlib by best elapsed time on
this generated-provider Alpha158DL setup. This comparison measures Qlib through
its real `Alpha158DL` data-handler path over a generated local Qlib binary
provider, not just isolated arithmetic kernels.

## KunQuant WorldQuant101

Command:

```powershell
uv run --frozen --with KunQuant --with setuptools python scripts\bench_factor_engines.py --workload worldquant101 --engines qfactors,kunquant --symbols 3000 --days 400 --repeats 1 --warmups 0 --threads 1
```

Results:

| engine | workload | rows | factors | best_s | cells/s | compile_s |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| qfactors | worldquant101 | 1,200,000 | 82 | 4.7886 | 20,548,602 | - |
| kunquant | worldquant101 | 1,200,000 | 82 | 32.6557 | 3,013,254 | 19.0082 |

Interpretation: qfactors is about 6.8x faster than KunQuant on end-to-end
compile-plus-run time. If KunQuant's observed compile phase is subtracted,
KunQuant runtime is about 13.65 seconds, and qfactors is still about 2.85x
faster on this run.

The current KunQuant package exposes 82 of the WorldQuant Alpha101 formulas used
by this benchmark. The KunQuant timing includes JIT compilation because that is
the end-to-end cost users pay when compiling a fresh expression bundle.

## Reproduction Options

Useful script options:

- `--symbols` and `--days` scale the synthetic panel.
- `--repeats` and `--warmups` control timing runs.
- `--names` selects a comma-separated factor subset.
- `--json results.json` saves machine-readable results.

The benchmark script is [scripts/bench_factor_engines.py](../scripts/bench_factor_engines.py).
