# 基准测试

[English](benchmark.en.md)

本文记录 qweave 的 benchmark 方法和复现命令。由于项目开发环境已经从 macOS
迁移到 Windows，历史性能数字不再作为当前参考；需要发布性能结论时，应在当前
Windows/PowerShell 环境重新测量并记录日期、机器配置和命令。

benchmark 脚本使用确定性的合成 OHLCV 面板，不依赖真实市场数据。

## 范围

当前 benchmark 关注有代表性的因子库执行路径：

- Qlib `Alpha158DL`，对比 qweave 的 `qlib_alpha158()` 表达式库。
- KunQuant Alpha101 JIT， 对比 qweave 的 `worldquant_alpha101()` 表达式库。

## 环境注意事项

- 命令使用 Windows PowerShell。
- Python 依赖通过 `uv` 管理。
- qweave 扩展应使用 release 模式构建：

```powershell
uv run maturin develop --uv --release
```

Qlib 和 KunQuant 的依赖/JIT 路径对非 ASCII 用户目录或 workspace 路径不够稳。
如果本机路径包含非 ASCII 字符，把临时目录指到 ASCII-only 路径：

```powershell
$env:TMP = "C:\qweave-bench-tmp"
$env:TEMP = "C:\qweave-bench-tmp"
```

Qlib 在全新 `uv run --with pyqlib` 环境里可能有首次 import、bytecode compile
和内部 cache 初始化成本。比较计算性能时至少使用 `--warmups 1`，避免把冷启动
开销混进计算时间。

## Qlib Alpha158

```powershell
uv run --frozen --with pyqlib python scripts\bench_factor_engines.py --workload alpha158 --engines qweave,qlib --symbols 6000 --days 800 --repeats 3 --warmups 1 --threads 1
```

记录结果时应包含：

- 日期和 commit SHA。
- Windows 版本、CPU、内存。
- symbols、days、repeats、warmups、threads。
- qweave、Qlib、Python、Rust toolchain 版本。
- best/mean/stdev、rows/s、cells/s。

## KunQuant WorldQuant101

```powershell
uv run --frozen --with KunQuant --with setuptools python scripts\bench_factor_engines.py --workload worldquant101 --engines qweave,kunquant --symbols 6000 --days 800 --repeats 3 --warmups 1 --threads 1
```

KunQuant 计时包含 JIT 编译，因为这是用户编译新表达式 bundle 时实际支付的
端到端成本。记录结果时同时保留 `compile_s`，便于区分 compile 和 run。

## 常用参数

- `--symbols` 和 `--days` 控制合成面板规模。
- `--repeats` 和 `--warmups` 控制计时次数。
- `--names` 选择逗号分隔的因子子集。
- `--json results.json` 保存机器可读结果。

脚本位置：[scripts/bench_factor_engines.py](../scripts/bench_factor_engines.py)。
