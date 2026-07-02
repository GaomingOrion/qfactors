import math

import polars as pl
import pytest
import qfactors


def test_roundtrip_dataframe():
    df = pl.DataFrame({"asset": ["A", "B"], "time": [1, 1], "close": [10.0, 20.0]})

    assert qfactors.roundtrip(df).equals(df)


def test_expression_collect_inputs_replace_inputs_alias_and_output_name():
    expr = ((qfactors.col("close") + qfactors.col("open")) / qfactors.lit(2.0)).alias("mid")

    assert expr.collect_inputs() == {"close", "open"}
    assert expr.output_name() == "mid"

    remapped = expr.replace_inputs({"close": "adj_close", "open": "adj_open"})
    df = pl.DataFrame(
        {
            "asset": ["A"],
            "time": [1],
            "adj_close": [12.0],
            "adj_open": [10.0],
        }
    )
    out = _with_alphas(df, [remapped])

    assert out.columns == ["asset", "time", "adj_close", "adj_open", "mid"]
    assert out.get_column("mid").to_list() == [11.0]


def test_alpha_executors_do_not_accept_column_aliases():
    df = _alpha_input_frame()
    expr = qfactors.col("close").alias("close_copy")

    with pytest.raises(TypeError, match="column_aliases"):
        qfactors.compute_alphas(
            df,
            symbol_col="asset",
            time_col="time",
            alphas=[expr],
            column_aliases={"close": "adj_close"},
        )
    with pytest.raises(TypeError, match="column_aliases"):
        qfactors.with_alphas(
            df,
            symbol_col="asset",
            time_col="time",
            alphas=[expr],
            column_aliases={"close": "adj_close"},
        )


def test_worldquant_alpha101_returns_expression_subset_with_aliases():
    selected = qfactors.worldquant_alpha101({}, alphas=["alpha13", "alpha101"])
    all_exprs = qfactors.worldquant_alpha101({})

    assert [expr.output_name() for expr in selected] == ["alpha13", "alpha101"]
    assert len(all_exprs) == 101
    assert {expr.output_name() for expr in all_exprs} == {f"alpha{idx}" for idx in range(1, 102)}
    assert selected[0].collect_inputs() == {"close", "volume"}
    assert "subindustry" in [
        expr for expr in all_exprs if expr.output_name() == "alpha100"
    ][0].collect_inputs()


def test_worldquant_alpha101_rejects_non_worldquant_names():
    with pytest.raises(ValueError, match="factor `group_returns_rank` is not known"):
        qfactors.worldquant_alpha101({}, alphas=["group_returns_rank"])
    with pytest.raises(ValueError, match="factor `alpha102` is not known"):
        qfactors.worldquant_alpha101({}, alphas=["alpha102"])


def test_compute_alphas_alpha101_matches_python_baseline():
    df = _alpha_input_frame()

    out = _compute_alphas(df, alphas=_alpha_exprs(["alpha101"]))

    assert out.columns == ["time", "asset", "alpha101"]
    assert out.select(["time", "asset"]).rows() == [
        (1, "A"),
        (1, "B"),
        (2, "A"),
        (2, "B"),
    ]
    expected = [
        _alpha101_baseline(df, 1, "A"),
        _alpha101_baseline(df, 1, "B"),
        _alpha101_baseline(df, 2, "A"),
        _alpha101_baseline(df, 2, "B"),
    ]
    for actual, expected_value in zip(out.get_column("alpha101").to_list(), expected):
        assert actual == pytest.approx(expected_value)


def test_compute_alphas_file_mode_matches_memory(tmp_path):
    df = _alpha_input_frame()
    output_path = tmp_path / "alpha_panel.parquet"

    alphas = _alpha_exprs(["alpha101"])
    memory = _compute_alphas(df, alphas=alphas)
    summary = _compute_alphas(
        df,
        alphas=alphas,
        output_path=str(output_path),
    )
    file_out = pl.read_parquet(output_path)

    assert summary == {
        "output_path": str(output_path),
        "n_observations": 1,
        "n_rows": memory.height,
    }
    assert file_out.equals(memory)


def test_with_alphas_mixes_custom_and_worldquant_exprs_in_original_order():
    df = _alpha_input_frame()
    custom = (
        (qfactors.col("close") - qfactors.col("open"))
        / (qfactors.col("high") - qfactors.col("low") + qfactors.lit(0.001))
    ).alias("custom")

    out = _with_alphas(df, [custom, *_alpha_exprs(["alpha101"])])

    assert out.select(["time", "asset"]).rows() == df.select(["time", "asset"]).rows()
    assert out.columns == [
        "asset",
        "time",
        "open",
        "close",
        "high",
        "low",
        "volume",
        "industry",
        "custom",
        "alpha101",
    ]
    for actual, expected in zip(out.get_column("custom").to_list(), out.get_column("alpha101").to_list()):
        assert actual == pytest.approx(expected)


def test_compute_alphas_worldquant_alpha101_representative_extra_fields_smoke():
    df = _worldquant_input_frame(n_times=40)

    out = _compute_alphas(
        df,
        alphas=_alpha_exprs(["alpha5", "alpha56", "alpha58", "alpha80"]),
    )

    assert out.columns == ["time", "asset", "alpha5", "alpha56", "alpha58", "alpha80"]
    assert out.filter(pl.col("time") == 40).select(["time", "asset"]).rows() == [
        (40, "A"),
        (40, "B"),
        (40, "C"),
        (40, "D"),
    ]


def test_new_rolling_methods_compute_expected_values():
    df = pl.DataFrame(
        {
            "asset": ["A"] * 5,
            "time": [1, 2, 3, 4, 5],
            "close": [1.0, 3.0, 5.0, 7.0, 8.0],
        }
    )
    alphas = [
        qfactors.col("close").slope(3).alias("slope"),
        qfactors.col("close").rsquare(3).alias("rsquare"),
        qfactors.col("close").resi(3).alias("resi"),
        qfactors.col("close").quantile(3, 0.8).alias("q80"),
    ]

    out = _compute_alphas(df, alphas=alphas)

    assert _nan_prefix(out.get_column("slope").to_list(), [2.0, 2.0, 1.5])
    assert _nan_prefix(out.get_column("rsquare").to_list(), [1.0, 1.0, 9.0 / (2.0 * (14.0 / 3.0))])
    assert _nan_prefix(out.get_column("resi").to_list(), [0.0, 0.0, -1.0 / 6.0])
    assert _nan_prefix(out.get_column("q80").to_list(), [4.2, 6.2, 7.6])


def _compute_alphas(df, alphas, output_path=None):
    return qfactors.compute_alphas(
        df,
        symbol_col="asset",
        time_col="time",
        alphas=alphas,
        output_path=output_path,
    )


def _with_alphas(df, alphas):
    return qfactors.with_alphas(
        df,
        symbol_col="asset",
        time_col="time",
        alphas=alphas,
    )


def _alpha_exprs(names):
    return qfactors.worldquant_alpha101({}, alphas=names)


def _alpha_input_frame():
    return pl.DataFrame(
        [
            {
                "asset": "B",
                "time": 2,
                "open": 21.0,
                "close": 24.0,
                "high": 25.0,
                "low": 20.0,
                "volume": 110.0,
                "industry": 1.0,
            },
            {
                "asset": "A",
                "time": 1,
                "open": 10.0,
                "close": 11.0,
                "high": 12.0,
                "low": 9.0,
                "volume": 100.0,
                "industry": 0.0,
            },
            {
                "asset": "A",
                "time": 2,
                "open": 12.0,
                "close": 15.0,
                "high": 16.0,
                "low": 11.0,
                "volume": 120.0,
                "industry": 0.0,
            },
            {
                "asset": "B",
                "time": 1,
                "open": 20.0,
                "close": 21.0,
                "high": 22.0,
                "low": 19.0,
                "volume": 90.0,
                "industry": 1.0,
            },
        ]
    )


def _worldquant_input_frame(n_times):
    rows = []
    for asset_idx, asset in enumerate(["A", "B", "C", "D"]):
        for time in range(1, n_times + 1):
            base = 10.0 * (asset_idx + 1) + time * 0.2
            close = base * (1.0 + ((time % 7) - 3) * 0.001)
            high = max(base, close) + 1.0 + asset_idx * 0.01
            low = min(base, close) - 1.0
            rows.append(
                {
                    "asset": asset,
                    "time": time,
                    "open": base,
                    "high": high,
                    "low": low,
                    "close": close,
                    "volume": 1_000.0 + asset_idx * 17.0 + time * 3.0,
                    "vwap": (high + low + close) / 3.0,
                    "cap": close * (1_000_000.0 + asset_idx * 100_000.0),
                    "sector": float(asset_idx % 2),
                    "industry": float(asset_idx % 2),
                    "subindustry": float(asset_idx % 2),
                }
            )
    return pl.DataFrame(rows)


def _alpha101_baseline(df, observation_time, asset):
    row = df.filter((pl.col("asset") == asset) & (pl.col("time") == observation_time)).row(
        0, named=True
    )
    return (row["close"] - row["open"]) / (row["high"] - row["low"] + 0.001)


def _nan_prefix(values, expected_tail):
    if not all(math.isnan(value) for value in values[:2]):
        return False
    for actual, expected in zip(values[2:], expected_tail):
        if actual != pytest.approx(expected):
            return False
    return True
