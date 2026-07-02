use std::collections::{BTreeMap, HashMap};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_polars::PyDataFrame;
use qfactors_core::{
    ComputeResult, ComputeSummary, Expr, PanelOptions, QFactorsError,
    compute_alphas as compute_alphas_core, with_alphas as with_alphas_core,
};

mod expr;
use expr::PyExpr;

// Rust-side allocations (the large per-node `Vec<f64>` buffers in the alpha engine) go
// through jemalloc on unix and mimalloc (v3, the crate default) on Windows. This only
// affects allocations made inside the extension module, not Python's own allocator.
#[cfg(not(target_os = "windows"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Return the input Polars DataFrame unchanged.
#[pyfunction]
fn roundtrip(df: PyDataFrame) -> PyDataFrame {
    df
}

/// Compute registered alpha expressions on a Polars panel.
///
/// The input DataFrame must contain the symbol and time columns plus every field
/// required by the requested alphas. The result always contains the full
/// (time, symbol) panel. Float input nulls become NaN; structural columns must
/// not contain nulls. If `output_path` is set, the result is written as Parquet
/// and a summary dict is returned. Otherwise a Polars DataFrame is returned.
#[pyfunction(name = "compute_alphas", signature = (
    df,
    symbol_col,
    time_col,
    alphas,
    output_path = None
))]
fn compute_alphas_py(
    py: Python<'_>,
    df: PyDataFrame,
    symbol_col: &str,
    time_col: &str,
    alphas: Vec<Py<PyExpr>>,
    output_path: Option<&str>,
) -> PyResult<Py<PyAny>> {
    let options = PanelOptions {
        symbol_col: symbol_col.to_string(),
        time_col: time_col.to_string(),
    };
    let alphas = alpha_specs_from_py(py, alphas).map_err(to_py_err)?;

    let result = compute_alphas_core(df.into(), options, alphas, output_path).map_err(to_py_err)?;
    match result {
        ComputeResult::Memory(df) => Ok(PyDataFrame(df).into_pyobject(py)?.unbind()),
        ComputeResult::File(summary) => summary_to_py(py, summary),
    }
}

/// Append alpha expression outputs to the input DataFrame in original row order.
#[pyfunction(name = "with_alphas", signature = (
    df,
    symbol_col,
    time_col,
    alphas
))]
fn with_alphas_py(
    py: Python<'_>,
    df: PyDataFrame,
    symbol_col: &str,
    time_col: &str,
    alphas: Vec<Py<PyExpr>>,
) -> PyResult<PyDataFrame> {
    let options = PanelOptions {
        symbol_col: symbol_col.to_string(),
        time_col: time_col.to_string(),
    };
    let alphas = alpha_specs_from_py(py, alphas).map_err(to_py_err)?;

    with_alphas_core(df.into(), options, alphas)
        .map(PyDataFrame)
        .map_err(to_py_err)
}

#[pyfunction(name = "worldquant_alpha101", signature = (input_alias, alphas = None))]
fn worldquant_alpha101_py(
    input_alias: HashMap<String, String>,
    alphas: Option<Vec<String>>,
) -> PyResult<Vec<PyExpr>> {
    alpha_builder_py(qfactors_factors::worldquant_alpha101(), input_alias, alphas)
}

#[pyfunction(name = "qlib_alpha158", signature = (input_alias, alphas = None))]
fn qlib_alpha158_py(
    input_alias: HashMap<String, String>,
    alphas: Option<Vec<String>>,
) -> PyResult<Vec<PyExpr>> {
    alpha_builder_py(qfactors_factors::qlib_alpha158(), input_alias, alphas)
}

fn alpha_builder_py(
    all: Vec<(String, Expr)>,
    input_alias: HashMap<String, String>,
    alphas: Option<Vec<String>>,
) -> PyResult<Vec<PyExpr>> {
    let input_alias: BTreeMap<String, String> = input_alias.into_iter().collect();
    let selected = match alphas {
        Some(names) => {
            let mut by_name = all.into_iter().collect::<HashMap<_, _>>();
            names
                .into_iter()
                .map(|name| {
                    let expr = by_name
                        .remove(&name)
                        .ok_or_else(|| QFactorsError::UnknownFactor(name.clone()))?;
                    Ok((name, expr))
                })
                .collect::<qfactors_core::Result<Vec<_>>>()
                .map_err(to_py_err)?
        }
        None => all,
    };

    Ok(selected
        .into_iter()
        .map(|(name, expr)| {
            PyExpr::named(
                &name,
                qfactors_core::expr::rename_fields(&expr, &input_alias),
            )
        })
        .collect())
}

fn summary_to_py(py: Python<'_>, summary: ComputeSummary) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("output_path", summary.output_path)?;
    dict.set_item("n_observations", summary.n_observations)?;
    dict.set_item("n_rows", summary.n_rows)?;
    Ok(dict.into_any().unbind())
}

fn alpha_specs_from_py(
    py: Python<'_>,
    alphas: Vec<Py<PyExpr>>,
) -> qfactors_core::Result<Vec<(String, Expr)>> {
    alphas
        .into_iter()
        .map(|alpha| {
            let alpha = alpha.borrow(py);
            let name = alpha
                .output_name_ref()
                .ok_or(QFactorsError::AlphaAliasRequired)?
                .to_string();
            Ok((name, alpha.expr()))
        })
        .collect()
}

#[pymodule]
fn qfactors(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    expr::register(module)?;
    module.add_function(wrap_pyfunction!(roundtrip, module)?)?;
    module.add_function(wrap_pyfunction!(compute_alphas_py, module)?)?;
    module.add_function(wrap_pyfunction!(with_alphas_py, module)?)?;
    module.add_function(wrap_pyfunction!(worldquant_alpha101_py, module)?)?;
    module.add_function(wrap_pyfunction!(qlib_alpha158_py, module)?)?;
    Ok(())
}

fn to_py_err(err: QFactorsError) -> PyErr {
    PyValueError::new_err(err.to_string())
}
