use std::collections::{BTreeSet, HashMap, HashSet};

use polars::prelude::*;

use crate::column_store::{ColumnStore, ensure_dtype, ensure_no_nulls};
use crate::compute_sink::MemorySink;
use crate::error::{QFactorsError, Result};
use crate::factor::{FactorDescriptor, FactorResult, ResolvedFactor};
use crate::obs_range_cache::ObsRangeCache;
use crate::prepared_panel::{GROUP_ID_COL, PreparedPanel};

pub fn compute_panel(
    panel: &PreparedPanel,
    observation_times: Series,
    factor_names: Vec<String>,
    output_path: Option<&str>,
    descriptors: &[FactorDescriptor],
) -> Result<DataFrame> {
    if output_path.is_some() {
        return Err(QFactorsError::UnsupportedOutputPath);
    }

    let columns = ColumnStore::new(panel.dataframe());
    let resolved = resolve_factors(panel, descriptors, &factor_names)?;
    let windows = collect_distinct_windows(&resolved)?;
    let observations = panel.resolve_observation_times(observation_times)?;
    let mut sink = MemorySink::new();

    for observation in observations {
        let range_cache = ObsRangeCache::new(panel, observation.ord_exclusive, &windows)?;
        let mut factor_columns = Vec::new();

        for factor in &resolved {
            let ranges = range_cache.ranges_for(factor.desc.window)?;
            let result = (factor.desc.compute)(&columns, ranges, factor)?;
            validate_factor_result(panel, factor, &result)?;
            factor_columns.extend(result);
        }

        let frame = panel.build_observation_frame(&observation, factor_columns)?;
        sink.write_observation(frame);
    }

    sink.finish()
}

fn resolve_factors<'a>(
    panel: &PreparedPanel,
    descriptors: &'a [FactorDescriptor],
    factor_names: &[String],
) -> Result<Vec<ResolvedFactor<'a>>> {
    let descriptor_by_name = descriptor_map(descriptors)?;
    let mut output_names = HashSet::new();
    let mut resolved = Vec::with_capacity(factor_names.len());

    for factor_name in factor_names {
        let desc = descriptor_by_name
            .get(factor_name.as_str())
            .ok_or_else(|| QFactorsError::UnknownFactor(factor_name.clone()))?;

        if desc.window == 0 {
            return Err(QFactorsError::InvalidWindow {
                factor_name: desc.factor_name,
                window: desc.window,
            });
        }

        let mut input_columns = Vec::with_capacity(desc.inputs.len());
        for input in desc.inputs {
            let column_name = panel
                .column_aliases()
                .get(input.name)
                .cloned()
                .unwrap_or_else(|| input.name.to_string());
            let column = panel
                .dataframe()
                .column(&column_name)
                .map_err(|_| QFactorsError::MissingColumn(column_name.clone()))?;
            ensure_dtype(column, input.dtype)?;
            ensure_no_nulls(column)?;
            input_columns.push(column_name);
        }

        let output_columns = default_output_columns(desc);
        for output_column in &output_columns {
            ensure_output_name_available(panel, &mut output_names, output_column)?;
        }

        resolved.push(ResolvedFactor {
            desc,
            input_columns,
            output_columns,
        });
    }

    Ok(resolved)
}

fn descriptor_map<'a>(
    descriptors: &'a [FactorDescriptor],
) -> Result<HashMap<&'static str, &'a FactorDescriptor>> {
    let mut by_name = HashMap::with_capacity(descriptors.len());
    for desc in descriptors {
        if by_name.insert(desc.factor_name, desc).is_some() {
            return Err(QFactorsError::OutputColumnConflict(
                desc.factor_name.to_string(),
            ));
        }
    }
    Ok(by_name)
}

fn default_output_columns(desc: &FactorDescriptor) -> Vec<String> {
    if desc.outputs.len() == 1 {
        vec![desc.factor_name.to_string()]
    } else {
        desc.outputs
            .iter()
            .map(|output| format!("{}.{}", desc.factor_name, output.name))
            .collect()
    }
}

fn ensure_output_name_available(
    panel: &PreparedPanel,
    seen: &mut HashSet<String>,
    name: &str,
) -> Result<()> {
    if name == panel.time_col()
        || name == panel.group_col()
        || (panel.output_group_id() && name == GROUP_ID_COL)
        || !seen.insert(name.to_string())
    {
        return Err(QFactorsError::OutputColumnConflict(name.to_string()));
    }
    Ok(())
}

fn collect_distinct_windows(factors: &[ResolvedFactor<'_>]) -> Result<Vec<usize>> {
    let mut windows = BTreeSet::new();
    for factor in factors {
        if factor.desc.window == 0 {
            return Err(QFactorsError::InvalidWindow {
                factor_name: factor.desc.factor_name,
                window: factor.desc.window,
            });
        }
        windows.insert(factor.desc.window);
    }
    Ok(windows.into_iter().collect())
}

fn validate_factor_result(
    panel: &PreparedPanel,
    factor: &ResolvedFactor<'_>,
    result: &FactorResult,
) -> Result<()> {
    if result.len() != factor.output_columns.len() {
        return Err(QFactorsError::FactorOutputCount {
            factor_name: factor.desc.factor_name,
            expected: factor.output_columns.len(),
            actual: result.len(),
        });
    }

    for (column, expected_name) in result.iter().zip(&factor.output_columns) {
        if column.len() != panel.groups().len() {
            return Err(QFactorsError::FactorOutputLength {
                factor_name: factor.desc.factor_name,
                column: column.name().to_string(),
                expected: panel.groups().len(),
                actual: column.len(),
            });
        }

        if column.name().as_str() != expected_name {
            return Err(QFactorsError::FactorOutputName {
                factor_name: factor.desc.factor_name,
                expected: expected_name.clone(),
                actual: column.name().to_string(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use super::*;
    use crate::factor::{ColumnSpec, DType};
    use crate::{NullPolicy, PreparePanelOptions};

    static INPUTS: [ColumnSpec; 1] = [ColumnSpec {
        name: "close",
        dtype: DType::F64,
    }];
    static OUTPUTS: [ColumnSpec; 1] = [ColumnSpec {
        name: "dummy",
        dtype: DType::F64,
    }];

    fn dummy_descriptor() -> FactorDescriptor {
        FactorDescriptor {
            factor_name: "dummy",
            kernel_name: "dummy",
            window: 2,
            inputs: &INPUTS,
            outputs: &OUTPUTS,
            param_set: None,
            params: &[],
            compute: dummy_compute,
        }
    }

    fn dummy_compute(
        columns: &ColumnStore<'_>,
        ranges: &[Option<Range<usize>>],
        factor: &ResolvedFactor<'_>,
    ) -> Result<FactorResult> {
        let close = columns.f64(&factor.input_columns[0])?;
        let mut values = vec![f64::NAN; ranges.len()];
        for (idx, range) in ranges.iter().enumerate() {
            if let Some(range) = range {
                values[idx] = close[range.end - 1] - close[range.start];
            }
        }
        Ok(vec![Column::new(
            factor.output_columns[0].clone().into(),
            values,
        )])
    }

    fn panel() -> Result<PreparedPanel> {
        let df = df!(
            "asset" => ["B", "A", "A", "B", "A"],
            "time" => [1i64, 3, 1, 3, 2],
            "close" => [20.0, 12.0, 10.0, 22.0, 11.0],
        )?;
        PreparedPanel::new(
            df,
            PreparePanelOptions {
                group_col: "asset".to_string(),
                time_col: "time".to_string(),
                column_aliases: HashMap::new(),
                sort: true,
                rechunk: true,
                null_policy: NullPolicy::Error,
                output_group_id: false,
            },
        )
    }

    #[test]
    fn computes_memory_panel_in_observation_then_group_order() -> Result<()> {
        let panel = panel()?;
        let out = compute_panel(
            &panel,
            Series::new("time".into(), [2i64, 3]),
            vec!["dummy".to_string()],
            None,
            &[dummy_descriptor()],
        )?;

        assert_eq!(out.height(), 4);
        assert_eq!(
            out.column("asset")?
                .try_str()
                .expect("asset is string")
                .iter()
                .map(|value| value.expect("asset has no nulls"))
                .collect::<Vec<_>>(),
            ["A", "B", "A", "B"]
        );

        let values = out
            .column("dummy")?
            .try_f64()
            .expect("dummy is f64")
            .into_no_null_iter()
            .collect::<Vec<_>>();
        assert_eq!(values[0], 1.0);
        assert!(values[1].is_nan());
        assert_eq!(values[2], 1.0);
        assert_eq!(values[3], 2.0);
        Ok(())
    }

    #[test]
    fn resolve_rejects_unknown_factor() {
        let panel = panel().unwrap();
        let err = compute_panel(
            &panel,
            Series::new("time".into(), [2i64]),
            vec!["missing".to_string()],
            None,
            &[dummy_descriptor()],
        )
        .unwrap_err();

        assert!(matches!(err, QFactorsError::UnknownFactor(_)));
    }
}
