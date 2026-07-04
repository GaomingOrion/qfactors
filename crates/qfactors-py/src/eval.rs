use std::path::{Path, PathBuf};
use std::process::Command;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3_polars::PyDataFrame;
use qfactors_core::PanelOptions;
use qfactors_eval::{
    Binning, Demean, EvalOutput, EvaluateOptions, TableData, Weighting, evaluate as evaluate_core,
    factor_correlation as factor_correlation_core, save_output, to_html as to_html_core,
};

/// Result object for `evaluate`: Polars tables plus the parameter snapshot.
///
/// In memory mode every table is a `polars.DataFrame`; with `output_dir` set,
/// the large tables (`ic`, `quantile_returns`, `coverage`) are returned as
/// `polars.LazyFrame` scans over the streamed parquet files.
#[pyclass(name = "EvalResult", frozen)]
pub struct PyEvalResult {
    output: EvalOutput,
}

#[pymethods]
impl PyEvalResult {
    /// One row per (factor, horizon): IC/RankIC statistics, top-bottom spread,
    /// monotonicity, and coverage.
    #[getter]
    fn summary(&self) -> PyDataFrame {
        PyDataFrame(self.output.summary.clone())
    }

    /// Daily IC and RankIC: date, factor, horizon, ic, rank_ic.
    #[getter]
    fn ic(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        table_to_py(py, &self.output.ic)
    }

    /// Daily per-bucket rows: date, factor, bin, bin_lo, bin_hi, count,
    /// mean_ret_{h}...
    #[getter]
    fn quantile_returns(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        table_to_py(py, &self.output.quantile_returns)
    }

    /// Daily sample accounting per factor: date, factor, n_valid, n_masked.
    #[getter]
    fn coverage(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        table_to_py(py, &self.output.coverage)
    }

    /// Daily top/bottom quantile turnover: date, factor, horizon,
    /// top_turnover, bottom_turnover.
    #[getter]
    fn turnover(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        table_to_py(py, &self.output.turnover)
    }

    /// Staggered long-short portfolio: date, factor, horizon, gross, net,
    /// turnover (needs a ret_1 label; NaN otherwise).
    #[getter]
    fn portfolio(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        table_to_py(py, &self.output.portfolio)
    }

    /// Time-mean factor rank autocorrelation: factor, lag, rank_autocorr.
    #[getter]
    fn rank_autocorr(&self) -> PyDataFrame {
        PyDataFrame(self.output.rank_autocorr.clone())
    }

    /// Monthly IC means (only when the time column is Date/Datetime).
    #[getter]
    fn ic_monthly(&self) -> Option<PyDataFrame> {
        self.output.ic_monthly.clone().map(PyDataFrame)
    }

    /// Snapshot of every evaluation parameter, as a dict.
    #[getter]
    fn meta(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let json = py.import("json")?;
        Ok(json
            .call_method1("loads", (self.output.meta_json.as_str(),))?
            .unbind())
    }

    /// Write all tables plus meta.json to `dir` (memory mode only; streamed
    /// results already live in their output_dir).
    fn save(&self, py: Python<'_>, dir: &str) -> PyResult<()> {
        py.detach(|| save_output(&self.output, dir))
            .map_err(|err| PyValueError::new_err(err.to_string()))
    }

    /// Write a self-contained HTML report (sortable summary table + per-factor
    /// quantile-return and monthly-IC charts) to `path`. Memory mode only;
    /// `max_detail_factors` caps the drill-down bundle to bound file size.
    #[pyo3(signature = (path, max_detail_factors = 200))]
    fn to_html(&self, py: Python<'_>, path: &str, max_detail_factors: usize) -> PyResult<()> {
        py.detach(|| to_html_core(&self.output, path, max_detail_factors))
            .map_err(|err| PyValueError::new_err(err.to_string()))
    }

    /// Launch the interactive report server on this result and block until
    /// interrupted (Ctrl-C). Memory-mode results are saved to a temporary dir
    /// first; streamed results serve their existing `output_dir` in place.
    ///
    /// Requires the `qfactors-server` binary (build it with
    /// `cargo build -p qfactors-server`). It is located via the
    /// `QFACTORS_SERVER_BIN` env var, then `target/{release,debug}` under the
    /// current dir, then `qfactors-server` on `PATH`. Set `QFACTORS_SERVER_ASSETS`
    /// to the built `frontend/dist` to serve the UI (otherwise API only).
    #[pyo3(signature = (port = 8080, open_browser = true))]
    fn serve(&self, py: Python<'_>, port: u16, open_browser: bool) -> PyResult<()> {
        let (dir, temp) = self.data_dir()?;
        let result = py.detach(|| run_server(&dir, port, open_browser));
        if let Some(temp) = temp {
            std::fs::remove_dir_all(&temp).ok();
        }
        result.map_err(PyValueError::new_err)
    }

    fn __repr__(&self) -> String {
        format!(
            "EvalResult(summary_rows={}, mode={})",
            self.output.summary.height(),
            match self.output.ic {
                TableData::Memory(_) => "memory",
                TableData::File(_) => "streamed",
            },
        )
    }
}

impl PyEvalResult {
    /// Resolve the parquet directory to serve. Returns the dir and, for
    /// memory-mode results, the temp dir to clean up afterwards.
    fn data_dir(&self) -> PyResult<(PathBuf, Option<PathBuf>)> {
        match &self.output.ic {
            TableData::File(path) => {
                let dir = Path::new(path)
                    .parent()
                    .ok_or_else(|| PyValueError::new_err("streamed table has no parent dir"))?;
                Ok((dir.to_path_buf(), None))
            }
            TableData::Memory(_) => {
                let dir = std::env::temp_dir().join(format!(
                    "qfactors-report-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0)
                ));
                let dir_str = dir.to_string_lossy().into_owned();
                save_output(&self.output, &dir_str)
                    .map_err(|err| PyValueError::new_err(err.to_string()))?;
                Ok((dir.clone(), Some(dir)))
            }
        }
    }
}

/// Locate the server binary and run it against `dir`, blocking until it exits.
fn run_server(dir: &Path, port: u16, open_browser: bool) -> Result<(), String> {
    let bin = locate_server_bin()
        .ok_or_else(|| "qfactors-server binary not found; build it with \
             `cargo build -p qfactors-server` or set QFACTORS_SERVER_BIN"
            .to_string())?;
    let mut cmd = Command::new(bin);
    cmd.arg("--dir").arg(dir).arg("--port").arg(port.to_string());
    if let Ok(assets) = std::env::var("QFACTORS_SERVER_ASSETS") {
        cmd.arg("--assets").arg(assets);
    }
    if open_browser {
        cmd.arg("--open");
    }
    let status = cmd.status().map_err(|err| format!("launching qfactors-server: {err}"))?;
    if !status.success() {
        return Err(format!("qfactors-server exited with {status}"));
    }
    Ok(())
}

fn locate_server_bin() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("QFACTORS_SERVER_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let exe = if cfg!(windows) {
        "qfactors-server.exe"
    } else {
        "qfactors-server"
    };
    for profile in ["release", "debug"] {
        let candidate = Path::new("target").join(profile).join(exe);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // Fall back to PATH resolution by the OS.
    Some(PathBuf::from("qfactors-server"))
}

fn table_to_py(py: Python<'_>, table: &TableData) -> PyResult<Py<PyAny>> {
    match table {
        TableData::Memory(df) => Ok(PyDataFrame(df.clone()).into_pyobject(py)?.unbind()),
        TableData::File(path) => {
            let polars = py.import("polars")?;
            Ok(polars
                .call_method1("scan_parquet", (path.as_str(),))?
                .unbind())
        }
    }
}

/// Evaluate factor columns against `ret_{h}` label columns on a single panel
/// DataFrame (see `with_alphas` / `with_labels` for producing the inputs).
#[pyfunction(name = "evaluate", signature = (
    df,
    symbol_col,
    time_col,
    factor_cols,
    label_cols = None,
    quantiles = 10,
    binning = "daily",
    group_col = None,
    tradable_col = None,
    demean = "none",
    min_cs_count = 30,
    cost_bps = 0.0,
    weighting = "factor",
    factor_source = None,
    output_dir = None
))]
#[allow(clippy::too_many_arguments)]
pub fn evaluate_py(
    py: Python<'_>,
    df: PyDataFrame,
    symbol_col: &str,
    time_col: &str,
    factor_cols: Vec<String>,
    label_cols: Option<Vec<String>>,
    quantiles: usize,
    binning: &str,
    group_col: Option<String>,
    tradable_col: Option<String>,
    demean: &str,
    min_cs_count: usize,
    cost_bps: f64,
    weighting: &str,
    factor_source: Option<String>,
    output_dir: Option<String>,
) -> PyResult<PyEvalResult> {
    let panel = PanelOptions {
        symbol_col: symbol_col.to_string(),
        time_col: time_col.to_string(),
    };
    let binning = match binning {
        "daily" => Binning::Daily,
        "global" => Binning::Global,
        other => {
            return Err(PyValueError::new_err(format!(
                "binning must be \"daily\" or \"global\"; got {other:?}"
            )));
        }
    };
    let demean = match demean {
        "none" => Demean::None,
        "universe" => Demean::Universe,
        "group" => Demean::Group,
        other => {
            return Err(PyValueError::new_err(format!(
                "demean must be \"none\", \"universe\", or \"group\"; got {other:?}"
            )));
        }
    };
    let weighting = match weighting {
        "factor" => Weighting::Factor,
        "quantile" => Weighting::Quantile,
        other => {
            return Err(PyValueError::new_err(format!(
                "weighting must be \"factor\" or \"quantile\"; got {other:?}"
            )));
        }
    };
    let options = EvaluateOptions {
        factor_cols,
        label_cols,
        quantiles,
        binning,
        demean,
        min_cs_count,
        group_col,
        tradable_col,
        cost_bps,
        weighting,
        factor_source,
        output_dir,
    };

    let output = py
        .detach(move || evaluate_core(&df.into(), &panel, &options))
        .map_err(|err| PyValueError::new_err(err.to_string()))?;
    Ok(PyEvalResult { output })
}

/// Time-averaged daily cross-sectional rank correlation between factors
/// (pairwise complete observations). Intended for the filtered shortlist
/// after `evaluate`: every factor column is held densely in memory.
#[pyfunction(name = "factor_correlation", signature = (
    df,
    symbol_col,
    time_col,
    factor_cols,
    tradable_col = None,
    min_cs_count = 30
))]
pub fn factor_correlation_py(
    py: Python<'_>,
    df: PyDataFrame,
    symbol_col: &str,
    time_col: &str,
    factor_cols: Vec<String>,
    tradable_col: Option<String>,
    min_cs_count: usize,
) -> PyResult<PyDataFrame> {
    let panel = PanelOptions {
        symbol_col: symbol_col.to_string(),
        time_col: time_col.to_string(),
    };
    py.detach(move || {
        factor_correlation_core(
            &df.into(),
            &panel,
            &factor_cols,
            tradable_col.as_deref(),
            min_cs_count,
        )
    })
    .map(PyDataFrame)
    .map_err(|err| PyValueError::new_err(err.to_string()))
}
