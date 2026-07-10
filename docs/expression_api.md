# Python 表达式 API

[English](expression_api.en.md)

qweave 提供 eager expression API，用于快速构造和执行 alpha 表达式。表达式是
Python 对象，底层由 Rust `Expr` 树表示；执行器会立即计算表达式列表。

## 构造表达式

```python
import qweave as qf

intraday_return = (
    (qf.col("close") - qf.col("open"))
    / (qf.col("high") - qf.col("low") + qf.lit(0.001))
).alias("intraday_return")
```

表达式传给 `compute_alphas` 或 `with_alphas` 前必须设置 alias；alias 会成为输出
列名。

常用操作包括：

- 算术：`+`、`-`、`*`、`/`、一元 `-`
- 比较：`<`、`>`、`<=`、`>=`、`==`
- 一元变换：`abs`、`log`、`sign`、`rank`、`scale`
- 时序窗口：`delay`、`delta`、`ts_sum`、`ts_mean`、`product`、`ts_min`、
  `ts_max`、`ts_argmin`、`ts_argmax`、`ts_rank`、`ts_rank_raw`、`ts_std`、
  `slope`、`rsquare`、`resi`、`quantile`、`decay_linear`
- 二元/多元函数：`min`、`max`、`power`、`signed_power`、`correlation`、
  `covariance`、`group_rank`、`group_neutralize`、`where_`

## 执行表达式

保留输入 DataFrame 并按原始行序追加因子列时，使用 `with_alphas`：

```python
out = qf.with_alphas(df, "asset", "time", [intraday_return])
```

需要完整历史的 `(time, symbol)` tidy panel 时，使用 `compute_alphas`：

```python
out = qf.compute_alphas(df, "asset", "time", [intraday_return])
```

`compute_alphas(..., output_path="alphas.parquet")` 会写出完整结果并返回摘要。
`with_alphas` 每个表达式会分配一个完整输出 buffer，再 scatter 回输入行序；大批量
因子且不需要保留原始 shape 时，优先使用 `compute_alphas`。

## 复用模板

`collect_inputs()` 返回表达式引用的标准输入字段，`replace_inputs()` 把这些字段
映射到实际 DataFrame 列，同时保留表达式 alias：

```python
expr = ((qf.col("close") + qf.col("open")) / qf.lit(2.0)).alias("mid")
assert expr.collect_inputs() == {"close", "open"}

adjusted = expr.replace_inputs({"close": "adj_close", "open": "adj_open"})
```

字段映射是表达式树的一部分。可见的 alias 路径只有 `replace_inputs()`，或内置
因子库的 `input_alias` 参数。

## 内置因子库

```python
alphas = qf.worldquant_alpha101(
    {"close": "adj_close", "open": "adj_open"},
    alphas=["alpha13", "alpha101"],
)
out = qf.compute_alphas(df, "asset", "time", alphas)
```

`qf.qlib_alpha158(input_alias, alphas=None)` 以同样签名暴露 Qlib Alpha158。
如果不需要字段映射，传入空 dict。实现口径和输入字段见
[WorldQuant 101](worldquant_alpha101.md) 与 [Qlib Alpha158](qlib_alpha158.md)。
