# 性能与基准测试

[English](benchmark.en.md)

qweave 面向大面板因子研究：在 Polars DataFrame 上一次性计算并返回因子结果，不把
研究代码拆成 Python 循环、临时 NumPy buffer 或编译产物管理。本页只发布当前
Windows 环境重新测量的结果，并保留机器、commit 和命令。

## 本次结论

- **对 Qlib Alpha158DL：** 完整 Alpha158（158 因子）上，qweave 最佳耗时快
  **23.85×**，峰值 RSS 少约 **46%**。
- **对 KunQuant Alpha101：** 在 KunQuant 支持的 82 个 WorldQuant101 因子、f64、
  关闭低精度 fast-stat、并把输入/输出转换与 JIT 都计入后，qweave 端到端最佳耗时快
  **2.56×**，峰值 RSS 少约 **31%**。
- **工作流：** qweave 直接接收并返回 Polars DataFrame。KunQuant 需将面板转换为
  `[time, stocks]` 的连续 NumPy 数组、编译 C++ bundle，再将输出数组字典转回 Polars
  DataFrame；这些步骤都已计时。Qlib Alpha158DL 已直接返回 pandas DataFrame，但使用
  本地 binary provider 与 handler 工作流。
- **截面因子：** qweave 的 `worldquant_alpha101()` 将截面与时序算子放在同一表达式
  DAG 中，通过一次 `compute_alphas` 直接在 Polars 面板上运行。相比围绕 provider /
  handler 组织的 Alpha158 路径，研究代码不需要先准备 provider、再通过 handler 分阶段
  取得特征后另行组织这类计算。

Qlib 与 KunQuant 支持的内置因子集不同，因而没有把两者的绝对秒数相互排名：Qlib 比较
只针对 Alpha158，KunQuant 比较只针对其支持的 82 个 Alpha101 因子。

## 测量环境与口径

- Windows 11 Pro 10.0.26200；AMD Ryzen 9 9950X，16 核 / 32 逻辑处理器；61.7 GiB
  内存；Python 3.12.13；Rust 1.99.0-nightly release 扩展。
- 确定性合成 OHLCV 面板：6,000 个股票 × 800 天 = 4,800,000 行；每项 1 次 warmup
  后测量 3 次，报告 best、mean、stdev 与进程峰值 RSS。
- `POLARS_MAX_THREADS=32` 与 `RAYON_NUM_THREADS=32`；Qlib 和 KunQuant 均传入
  `--threads 32`。KunQuant 在 x64 MSVC 环境运行。
- Alpha158 于 2026-07-10、commit [`eb8c5d5`](https://github.com/GaomingOrion/qweave/commit/eb8c5d5)
  测量；WorldQuant101 于 2026-07-11、commit [`ecbe2d7`](https://github.com/GaomingOrion/qweave/commit/ecbe2d7)
  测量。

## qweave vs Qlib：Alpha158 全部 158 因子

Qlib `Alpha158DL` 与 qweave `qlib_alpha158()` 都产出完整 Alpha158。Qlib 使用由同一
合成面板生成的本地 binary provider；其 loader 直接返回 pandas DataFrame，脚本没有做
额外输出转换。

| 引擎 | 最佳耗时 | 平均耗时 ± stdev | 因子值/秒 | 进程峰值 RSS |
| --- | ---: | ---: | ---: | ---: |
| qweave | **2.2379 s** | 2.3092 ± 0.0619 s | 338,887,623 | 10,324.1 MiB |
| Qlib Alpha158DL | 53.3692 s | 54.2714 ± 0.7821 s | 14,210,453 | 19,054.9 MiB |

按最佳耗时，qweave 快 **23.85×**；峰值内存为 Qlib 的约 54%。这反映的是此 Alpha158
provider/handler 路径的结果，不代表 Qlib 平台全部功能的性能边界。

## qweave vs KunQuant：WorldQuant101 的 82 因子子集

两条路径计算 KunQuant 支持的同一 82 个 Alpha101 因子。KunQuant 编译为 f64
(`double`)，关闭 `fast_stat`，并使用 f64 输入。其端到端时间包含 Polars 面板排序、转换为
TS NumPy 数组、JIT 编译、`runGraph`、以及输出数组字典转换为 4,800,000 行 × 84 列的
Polars DataFrame；`output_convert_s` 单列最佳输出转换耗时。

| 引擎 | 最佳耗时 | 平均耗时 ± stdev | 因子值/秒 | 进程峰值 RSS | 最佳编译 | 最佳输出转换 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| qweave | **3.1106 s** | 3.2749 ± 0.1449 s | 126,535,428 | 9,921.3 MiB | — | — |
| KunQuant f64 | 7.9515 s | 8.0905 ± 0.2087 s | 49,500,299 | 14,324.0 MiB | 3.0587 s | 0.6430 s |

端到端 qweave 快 **2.56×**。KunQuant 的 3.0587 s 编译与 0.6430 s 输出 DataFrame 转换
均是实际研究工作流的一部分；即使扣除编译，剩余时间仍混有输入转换、执行和输出转换，
不能称为“纯计算”基准。qweave 不需要 C++ 工具链、JIT 等待或编译产物管理，也无需在
DataFrame 与 NumPy buffer 之间组织来回转换。

## qweave 的实现优势

- **Rust + 共享 DAG：** 排序、rolling window、截面算子与 DAG evaluator 在 Rust 侧运行；
  多个因子共享子表达式时会复用结果，而非逐因子重复求值。
- **内存生命周期管理：** evaluator 复用中间 slot，减少不必要的分配；两组实测的峰值
  RSS 都低于对手。
- **DataFrame 原生接口：** 输入和输出均为 Polars DataFrame，适合直接接到清洗、标签、
  评估和下游研究流程。
- **一条表达式路径覆盖时序与截面：** WorldQuant101 的时序和截面因子可同批计算，不必将
  因子实现拆到独立的 provider / handler 阶段。

这些是当前版本、这台机器和指定合成数据上的结果，不是跨版本、跨硬件或全部因子集合的
固定承诺。请在目标环境复测。

## 在你的机器上复现

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

在 x64 Developer PowerShell 中运行 KunQuant：

```powershell
uv run --frozen --with KunQuant --with setuptools python scripts\bench_factor_engines.py --workload worldquant101 --engines qweave,kunquant --symbols 6000 --days 800 --repeats 3 --warmups 1 --threads 32 --json results-worldquant101.json
```

`--symbols`、`--days`、`--repeats`、`--warmups`、`--names` 与 `--threads` 控制基准
规模和并行度。JSON 会记录 `compile_seconds` 和 `output_conversion_seconds`（KunQuant
适用）。脚本位置：[scripts/bench_factor_engines.py](../scripts/bench_factor_engines.py)。
