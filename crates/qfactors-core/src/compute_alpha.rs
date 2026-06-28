use std::collections::{BTreeSet, HashSet};

use polars::prelude::*;

use crate::alpha_eval::{eval, to_grid};
use crate::alpha_registry::alpha_registry;
use crate::compute_panel::{ComputePanelOptions, reject_nan_values};
use crate::compute_sink::{ComputeResult, ComputeSink};
use crate::error::{QFactorsError, Result};
use crate::expr::{Expr, collect_fields};
use crate::grid::{Axis, GridCtx, build_grid};

struct ResolvedAlphaObservations {
    values: Column,
    time_rows: Vec<Option<usize>>,
}

pub fn compute_alphas(
    df: DataFrame,
    options: ComputePanelOptions,
    alpha_names: Vec<String>,
    observation_times: Series,
    output_path: Option<&str>,
) -> Result<ComputeResult> {
    let resolved = resolve_alphas(&options, alpha_names)?;
    let mut fields = BTreeSet::new();
    for (_, expr) in &resolved {
        collect_fields(expr, &mut fields);
    }

    let (ctx, symbols, times) = build_grid(&df, &options, &fields)?;
    let results = resolved
        .into_iter()
        .map(|(name, expr)| Ok((name, to_grid(eval(&expr, &ctx)?, &ctx))))
        .collect::<Result<Vec<_>>>()?;
    let observations =
        resolve_alpha_observations(&df, &options.time_col, &times, observation_times)?;
    let symbol_column = df
        .column(&options.symbol_col)
        .map_err(|_| QFactorsError::MissingColumn(options.symbol_col.clone()))?;

    let mut sink = ComputeSink::for_output(output_path);
    for (input_index, time_row) in observations.time_rows.iter().enumerate() {
        let frame = build_observation_frame(
            &ctx,
            &symbols,
            symbol_column,
            &results,
            &observations.values,
            input_index,
            *time_row,
            &options,
        )?;
        sink.write_observation(frame)?;
    }

    sink.finish()
}

fn resolve_alphas(
    options: &ComputePanelOptions,
    alpha_names: Vec<String>,
) -> Result<Vec<(String, Expr)>> {
    let registry = alpha_registry()?;
    let mut output_names = HashSet::new();
    let mut resolved = Vec::with_capacity(alpha_names.len());

    for name in alpha_names {
        ensure_output_name_available(options, &mut output_names, &name)?;
        let descriptor = registry
            .get(&name)
            .ok_or_else(|| QFactorsError::UnknownFactor(name.clone()))?;
        resolved.push((name, (descriptor.build)()));
    }

    Ok(resolved)
}

fn ensure_output_name_available(
    options: &ComputePanelOptions,
    seen: &mut HashSet<String>,
    name: &str,
) -> Result<()> {
    if name == options.time_col || name == options.symbol_col || !seen.insert(name.to_string()) {
        return Err(QFactorsError::OutputColumnConflict(name.to_string()));
    }
    Ok(())
}

#[allow(clippy::mutable_key_type)]
fn resolve_alpha_observations(
    df: &DataFrame,
    time_col: &str,
    times: &Axis,
    observation_times: Series,
) -> Result<ResolvedAlphaObservations> {
    let time_dtype = df.column(time_col)?.dtype().clone();
    let mut values = observation_times.cast(&time_dtype)?.into_column();
    values.rename(time_col.into());

    if values.is_empty() {
        return Err(QFactorsError::ObservationTimesEmpty);
    }
    if values.null_count() > 0 {
        return Err(QFactorsError::ObservationTimeNull);
    }
    reject_nan_values(&values)?;

    let mut seen = HashSet::with_capacity(values.len());
    let mut time_rows = Vec::with_capacity(values.len());
    for row in 0..values.len() {
        let value = values.get(row)?.into_static();
        if !seen.insert(value.clone()) {
            return Err(QFactorsError::DuplicateObservationTime(format!(
                "{value:?}"
            )));
        }
        time_rows.push(times.index_by_value.get(&value).copied());
    }

    Ok(ResolvedAlphaObservations { values, time_rows })
}

#[allow(clippy::too_many_arguments)]
fn build_observation_frame(
    ctx: &GridCtx,
    symbols: &Axis,
    source_symbol: &Column,
    results: &[(String, Vec<f64>)],
    observation_values: &Column,
    input_index: usize,
    time_row: Option<usize>,
    options: &ComputePanelOptions,
) -> Result<DataFrame> {
    let present_symbols = match time_row {
        Some(time_row) => (0..ctx.n_symbols)
            .filter(|&symbol| ctx.presence[symbol * ctx.n_times + time_row])
            .collect::<Vec<_>>(),
        None => Vec::new(),
    };
    let n_rows = present_symbols.len();

    let mut time = observation_values.new_from_index(input_index, n_rows);
    time.rename(options.time_col.clone().into());

    let mut symbol = if present_symbols.is_empty() {
        Column::new_empty(options.symbol_col.clone().into(), source_symbol.dtype())
    } else {
        let row_indices = present_symbols
            .iter()
            .map(|symbol| symbols.source_rows[*symbol] as IdxSize)
            .collect::<Vec<_>>();
        source_symbol.take_slice(&row_indices)?
    };
    symbol.rename(options.symbol_col.clone().into());

    let mut columns = vec![time, symbol];
    for (name, values) in results {
        let column = match time_row {
            Some(time_row) if !present_symbols.is_empty() => Column::new(
                name.clone().into(),
                present_symbols
                    .iter()
                    .map(|symbol| values[*symbol * ctx.n_times + time_row])
                    .collect::<Vec<_>>(),
            ),
            _ => Column::new_empty(name.clone().into(), &DataType::Float64),
        };
        columns.push(column);
    }

    Ok(DataFrame::new_infer_height(columns)?)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use linkme::distributed_slice;

    use super::*;
    use crate::alpha_registry::{ALPHA_DESCRIPTORS, AlphaDescriptor};

    fn test_alpha_build() -> Expr {
        Expr::Field("close".to_string())
    }

    fn test_alpha_descriptor() -> AlphaDescriptor {
        AlphaDescriptor {
            name: "test_alpha",
            build: test_alpha_build,
        }
    }

    #[distributed_slice(ALPHA_DESCRIPTORS)]
    static TEST_ALPHA_DESCRIPTOR: fn() -> AlphaDescriptor = test_alpha_descriptor;

    fn options() -> ComputePanelOptions {
        ComputePanelOptions {
            symbol_col: "asset".to_string(),
            time_col: "time".to_string(),
            column_aliases: HashMap::new(),
        }
    }

    fn memory_frame(result: ComputeResult) -> Result<DataFrame> {
        match result {
            ComputeResult::Memory(df) => Ok(df),
            ComputeResult::File(_) => panic!("expected memory result"),
        }
    }

    #[test]
    fn compute_alphas_samples_only_present_symbols() -> Result<()> {
        let df = df!(
            "asset" => ["B", "A", "A"],
            "time" => [2i64, 1, 2],
            "close" => [20.0, 10.0, 11.0],
        )?;

        let out = memory_frame(compute_alphas(
            df,
            options(),
            vec!["test_alpha".to_string()],
            Series::new("time".into(), [1i64, 2]),
            None,
        )?)?;

        assert_eq!(out.height(), 3);
        assert_eq!(
            out.column("test_alpha")?
                .try_f64()
                .expect("test_alpha is f64")
                .into_no_null_iter()
                .collect::<Vec<_>>(),
            [10.0, 11.0, 20.0]
        );
        Ok(())
    }

    #[test]
    fn missing_observation_time_preserves_schema() -> Result<()> {
        let df = df!(
            "asset" => ["A"],
            "time" => [1i64],
            "close" => [10.0],
        )?;

        let out = memory_frame(compute_alphas(
            df,
            options(),
            vec!["test_alpha".to_string()],
            Series::new("time".into(), [9i64]),
            None,
        )?)?;

        assert_eq!(out.height(), 0);
        assert_eq!(
            out.get_column_names()
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>(),
            ["time", "asset", "test_alpha"]
        );
        Ok(())
    }
}
