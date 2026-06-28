use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use polars::prelude::*;

use crate::column_store::ensure_dtype;
use crate::compute_panel::{ComputePanelOptions, validate_structural_column};
use crate::error::{QFactorsError, Result};
use crate::factor::DType;

#[derive(Debug, Clone)]
pub struct GridCtx {
    pub n_symbols: usize,
    pub n_times: usize,
    pub fields: HashMap<String, Vec<f64>>,
    pub presence: Vec<bool>,
}

#[derive(Debug, Clone)]
pub struct Axis {
    pub values: Vec<AnyValue<'static>>,
    pub source_rows: Vec<usize>,
    pub index_by_value: HashMap<AnyValue<'static>, usize>,
}

pub fn build_grid(
    df: &DataFrame,
    options: &ComputePanelOptions,
    fields: &BTreeSet<String>,
) -> Result<(GridCtx, Axis, Axis)> {
    let symbol_col = df
        .column(&options.symbol_col)
        .map_err(|_| QFactorsError::MissingColumn(options.symbol_col.clone()))?;
    let time_col = df
        .column(&options.time_col)
        .map_err(|_| QFactorsError::MissingColumn(options.time_col.clone()))?;

    validate_structural_column(symbol_col, true)?;
    validate_structural_column(time_col, false)?;

    let symbols = build_axis(symbol_col)?;
    let times = build_axis(time_col)?;
    let size = symbols.values.len() * times.values.len();

    let mut field_inputs = Vec::with_capacity(fields.len());
    let mut field_values = HashMap::with_capacity(fields.len());
    for logical_name in fields {
        let column_name = options
            .column_aliases
            .get(logical_name)
            .cloned()
            .unwrap_or_else(|| logical_name.clone());
        let column = df
            .column(&column_name)
            .map_err(|_| QFactorsError::MissingColumn(column_name.clone()))?;
        ensure_dtype(column, DType::F64)?;
        field_inputs.push((
            logical_name.clone(),
            column.try_f64().expect("dtype checked above"),
        ));
        field_values.insert(logical_name.clone(), vec![f64::NAN; size]);
    }

    let mut presence = vec![false; size];
    for row in 0..df.height() {
        let symbol_value = symbol_col.get(row)?.into_static();
        let time_value = time_col.get(row)?.into_static();
        let symbol_idx = symbols.index_by_value[&symbol_value];
        let time_idx = times.index_by_value[&time_value];
        let grid_idx = symbol_idx * times.values.len() + time_idx;

        if presence[grid_idx] {
            return Err(QFactorsError::DuplicateSymbolTime {
                symbol_col: options.symbol_col.clone(),
                time_col: options.time_col.clone(),
            });
        }
        presence[grid_idx] = true;

        for (logical_name, column) in &field_inputs {
            field_values
                .get_mut(logical_name)
                .expect("field buffer initialized")[grid_idx] = column.get(row).unwrap_or(f64::NAN);
        }
    }

    Ok((
        GridCtx {
            n_symbols: symbols.values.len(),
            n_times: times.values.len(),
            fields: field_values,
            presence,
        },
        symbols,
        times,
    ))
}

#[allow(clippy::mutable_key_type)]
fn build_axis(column: &Column) -> Result<Axis> {
    let mut source_by_value = HashMap::new();
    for row in 0..column.len() {
        let value = column.get(row)?.into_static();
        source_by_value.entry(value).or_insert(row);
    }

    let mut values = source_by_value.keys().cloned().collect::<Vec<_>>();
    let mut sort_error = None;
    values.sort_by(|lhs, rhs| match lhs.partial_cmp(rhs) {
        Some(ordering) => ordering,
        None => {
            sort_error = Some(QFactorsError::NonComparableColumn {
                column: column.name().to_string(),
            });
            Ordering::Equal
        }
    });
    if let Some(err) = sort_error {
        return Err(err);
    }

    let source_rows = values
        .iter()
        .map(|value| source_by_value[value])
        .collect::<Vec<_>>();
    let index_by_value = values
        .iter()
        .cloned()
        .enumerate()
        .map(|(idx, value)| (value, idx))
        .collect();

    Ok(Axis {
        values,
        source_rows,
        index_by_value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> ComputePanelOptions {
        ComputePanelOptions {
            symbol_col: "asset".to_string(),
            time_col: "time".to_string(),
            column_aliases: HashMap::new(),
        }
    }

    #[test]
    fn build_grid_sorts_axes_scatters_fields_and_presence() -> Result<()> {
        let df = df!(
            "asset" => ["B", "A", "A"],
            "time" => [2i64, 1, 2],
            "open" => [20.0, 10.0, 11.0],
            "close" => [21.0, 10.5, 12.0],
        )?;
        let fields = BTreeSet::from(["open".to_string(), "close".to_string()]);

        let (ctx, symbols, times) = build_grid(&df, &options(), &fields)?;

        assert_eq!(ctx.n_symbols, 2);
        assert_eq!(ctx.n_times, 2);
        assert_eq!(
            symbols
                .values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["\"A\"", "\"B\""]
        );
        assert_eq!(
            times
                .values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
        assert_eq!(ctx.presence, [true, true, false, true]);
        assert_eq!(ctx.fields["open"][0], 10.0);
        assert_eq!(ctx.fields["open"][1], 11.0);
        assert!(ctx.fields["open"][2].is_nan());
        assert_eq!(ctx.fields["open"][3], 20.0);
        Ok(())
    }

    #[test]
    fn build_grid_uses_aliases_and_float_nulls_become_nan() -> Result<()> {
        let mut options = options();
        options
            .column_aliases
            .insert("open".to_string(), "raw_open".to_string());
        let df = df!(
            "asset" => ["A", "A"],
            "time" => [1i64, 2],
            "raw_open" => [Some(10.0), None],
        )?;
        let fields = BTreeSet::from(["open".to_string()]);

        let (ctx, _, _) = build_grid(&df, &options, &fields)?;

        assert_eq!(ctx.fields["open"][0], 10.0);
        assert!(ctx.fields["open"][1].is_nan());
        Ok(())
    }

    #[test]
    fn build_grid_rejects_duplicate_symbol_time() {
        let df = df!(
            "asset" => ["A", "A"],
            "time" => [1i64, 1],
            "open" => [10.0, 11.0],
        )
        .unwrap();
        let fields = BTreeSet::from(["open".to_string()]);

        let err = build_grid(&df, &options(), &fields).unwrap_err();

        assert!(matches!(err, QFactorsError::DuplicateSymbolTime { .. }));
    }

    #[test]
    fn build_grid_rejects_missing_wrong_dtype_and_structural_null() {
        let missing = df!(
            "asset" => ["A"],
            "time" => [1i64],
        )
        .unwrap();
        let fields = BTreeSet::from(["open".to_string()]);
        let err = build_grid(&missing, &options(), &fields).unwrap_err();
        assert!(matches!(err, QFactorsError::MissingColumn(_)));

        let wrong_dtype = df!(
            "asset" => ["A"],
            "time" => [1i64],
            "open" => [1i64],
        )
        .unwrap();
        let err = build_grid(&wrong_dtype, &options(), &fields).unwrap_err();
        assert!(matches!(err, QFactorsError::DTypeMismatch { .. }));

        let structural_null = df!(
            "asset" => [Some("A"), None],
            "time" => [1i64, 2],
            "open" => [10.0, 11.0],
        )
        .unwrap();
        let err = build_grid(&structural_null, &options(), &fields).unwrap_err();
        assert!(matches!(err, QFactorsError::SymbolNull(_)));
    }
}
