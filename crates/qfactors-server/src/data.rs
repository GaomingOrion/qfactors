//! Read a saved evaluation `output_dir` (one parquet per table + `meta.json`)
//! and shape it into JSON for the API. Reads are eager (matching the crate's
//! existing `ParquetReader` usage) and filtered per factor in memory — the
//! interactive report targets a shortlist, not thousand-factor runs.

use std::collections::HashSet;
use std::fs::File;
use std::path::PathBuf;

use polars::prelude::*;
use serde_json::{Map, Value};

/// A directory produced by `evaluate(output_dir=...)` / `EvalResult.save()`.
pub struct DataDir {
    root: PathBuf,
}

#[derive(Debug)]
pub enum DataError {
    Missing(String),
    Polars(PolarsError),
    Io(std::io::Error),
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataError::Missing(what) => write!(f, "{what}"),
            DataError::Polars(err) => write!(f, "polars: {err}"),
            DataError::Io(err) => write!(f, "io: {err}"),
        }
    }
}

impl std::error::Error for DataError {}

impl From<PolarsError> for DataError {
    fn from(err: PolarsError) -> Self {
        DataError::Polars(err)
    }
}
impl From<std::io::Error> for DataError {
    fn from(err: std::io::Error) -> Self {
        DataError::Io(err)
    }
}

type Result<T> = std::result::Result<T, DataError>;

impl DataDir {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        if !root.join("summary.parquet").is_file() {
            return Err(DataError::Missing(format!(
                "{} is not an evaluation output dir (no summary.parquet)",
                root.display()
            )));
        }
        Ok(Self { root })
    }

    fn path(&self, table: &str) -> PathBuf {
        self.root.join(format!("{table}.parquet"))
    }

    /// Read a table, optionally keeping only rows for one factor. Missing files
    /// (e.g. `ic_monthly` on an integer time axis) surface as `Missing`.
    fn read(&self, table: &str, factor: Option<&str>) -> Result<DataFrame> {
        let path = self.path(table);
        if !path.is_file() {
            return Err(DataError::Missing(format!("no {table} table")));
        }
        let mut df = ParquetReader::new(File::open(&path)?).finish()?;
        if let Some(name) = factor {
            let mask = df.column("factor")?.str()?.equal(name);
            df = df.filter(&mask)?;
        }
        Ok(stringify_dates(df)?)
    }

    /// Raw `meta.json` bytes (returned to the client verbatim).
    pub fn meta_json(&self) -> Result<String> {
        let path = self.root.join("meta.json");
        if !path.is_file() {
            return Err(DataError::Missing("no meta.json".to_string()));
        }
        Ok(std::fs::read_to_string(path)?)
    }

    /// Distinct factor names, in the summary's row order.
    pub fn factors(&self) -> Result<Vec<String>> {
        let df = self.read("summary", None)?;
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for value in df.column("factor")?.str()?.iter().flatten() {
            if seen.insert(value.to_string()) {
                out.push(value.to_string());
            }
        }
        Ok(out)
    }

    /// The full summary table as an array of row objects (drives the grid).
    pub fn summary_records(&self) -> Result<Value> {
        Ok(Value::Array(df_to_records(&self.read("summary", None)?)))
    }

    /// Every per-factor series the tearsheet needs, in one payload. All horizons
    /// are included; the frontend filters client-side so switching horizon needs
    /// no round-trip. `monthly` is null when there is no `ic_monthly` table.
    pub fn factor_bundle(&self, name: &str) -> Result<Value> {
        if !self.factors()?.iter().any(|f| f == name) {
            return Err(DataError::Missing(format!("unknown factor {name:?}")));
        }
        let mut obj = Map::new();
        obj.insert("factor".into(), Value::String(name.to_string()));
        obj.insert("ic".into(), df_columns(&self.read("ic", Some(name))?)?);
        obj.insert(
            "quantiles".into(),
            df_columns(&self.read("quantile_returns", Some(name))?)?,
        );
        obj.insert(
            "portfolio".into(),
            df_columns(&self.read("portfolio", Some(name))?)?,
        );
        obj.insert(
            "monthly".into(),
            match self.read("ic_monthly", Some(name)) {
                Ok(df) => df_columns(&df)?,
                Err(DataError::Missing(_)) => Value::Null,
                Err(err) => return Err(err),
            },
        );
        Ok(Value::Object(obj))
    }
}

/// Cast a `date` column (Date/Datetime) to ISO strings so the JSON carries axis
/// labels directly. No-op when the column is absent or already textual.
fn stringify_dates(df: DataFrame) -> PolarsResult<DataFrame> {
    if !df.get_column_names().iter().any(|c| c.as_str() == "date") {
        return Ok(df);
    }
    match df.column("date")?.dtype() {
        DataType::Date | DataType::Datetime(_, _) => {
            let mut df = df;
            let as_str = df.column("date")?.cast(&DataType::String)?;
            df.with_column(as_str)?;
            Ok(df)
        }
        _ => Ok(df),
    }
}

/// `{ colName: [values...] }` — compact column-oriented JSON for time series.
fn df_columns(df: &DataFrame) -> Result<Value> {
    let names: Vec<String> = df.get_column_names().iter().map(|n| n.to_string()).collect();
    let mut obj = Map::new();
    for name in &names {
        let column = df.column(name)?;
        let mut values = Vec::with_capacity(df.height());
        for row in 0..df.height() {
            values.push(any_value_json(column.get(row)?));
        }
        obj.insert(name.clone(), Value::Array(values));
    }
    Ok(Value::Object(obj))
}

/// `[ { col: value, ... }, ... ]` — row-oriented JSON for the summary grid.
fn df_to_records(df: &DataFrame) -> Vec<Value> {
    let names: Vec<String> = df.get_column_names().iter().map(|n| n.to_string()).collect();
    let mut rows = Vec::with_capacity(df.height());
    for row in 0..df.height() {
        let mut obj = Map::new();
        for name in &names {
            let value = df
                .column(name)
                .and_then(|c| c.get(row))
                .map(any_value_json)
                .unwrap_or(Value::Null);
            obj.insert(name.clone(), value);
        }
        rows.push(Value::Object(obj));
    }
    rows
}

fn any_value_json(value: AnyValue<'_>) -> Value {
    match value {
        AnyValue::Null => Value::Null,
        AnyValue::Boolean(v) => Value::Bool(v),
        AnyValue::Float64(v) => finite_json(v),
        AnyValue::Float32(v) => finite_json(v as f64),
        AnyValue::Int64(v) => Value::from(v),
        AnyValue::Int32(v) => Value::from(v),
        AnyValue::UInt64(v) => Value::from(v),
        AnyValue::UInt32(v) => Value::from(v),
        AnyValue::String(v) => Value::String(v.to_string()),
        AnyValue::StringOwned(v) => Value::String(v.to_string()),
        other => Value::String(other.to_string()),
    }
}

/// JSON has no NaN/Inf; map them to null so the frontend renders gaps.
fn finite_json(v: f64) -> Value {
    if v.is_finite() {
        serde_json::Number::from_f64(v).map(Value::Number).unwrap_or(Value::Null)
    } else {
        Value::Null
    }
}
