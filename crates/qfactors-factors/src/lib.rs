use std::ops::Range;

use polars::prelude::Column;
use qfactors_core::{
    ColumnSpec, ColumnStore, DType, FactorDescriptor, FactorResult, ResolvedFactor, Result,
};

static RET_INPUTS: [ColumnSpec; 2] = [
    ColumnSpec {
        name: "open",
        dtype: DType::F64,
    },
    ColumnSpec {
        name: "close",
        dtype: DType::F64,
    },
];

static RET_OUTPUTS: [ColumnSpec; 1] = [ColumnSpec {
    name: "ret",
    dtype: DType::F64,
}];

pub fn ensure_linked() {}

pub fn phase2_descriptors() -> Vec<FactorDescriptor> {
    vec![ret_descriptor()]
}

pub fn ret(open: &[f64], close: &[f64]) -> f64 {
    close[close.len() - 1] / open[0] - 1.0
}

pub fn ret_descriptor() -> FactorDescriptor {
    FactorDescriptor {
        factor_name: "ret",
        kernel_name: "ret",
        window: 60,
        inputs: &RET_INPUTS,
        outputs: &RET_OUTPUTS,
        param_set: None,
        params: &[],
        compute: ret_compute,
    }
}

fn ret_compute(
    columns: &ColumnStore<'_>,
    ranges: &[Option<Range<usize>>],
    factor: &ResolvedFactor<'_>,
) -> Result<FactorResult> {
    let open = columns.f64(&factor.input_columns[0])?;
    let close = columns.f64(&factor.input_columns[1])?;
    let mut values = vec![f64::NAN; ranges.len()];

    for (group_idx, range) in ranges.iter().enumerate() {
        if let Some(range) = range {
            values[group_idx] = ret(&open[range.clone()], &close[range.clone()]);
        }
    }

    Ok(vec![Column::new(
        factor.output_columns[0].clone().into(),
        values,
    )])
}

#[cfg(test)]
mod tests {
    use qfactors_core::{NullPolicy, PreparePanelOptions, PreparedPanel, compute_panel};
    use std::collections::HashMap;

    use polars::prelude::*;

    use super::*;

    #[test]
    fn ret_descriptor_computes_valid_and_insufficient_windows() -> qfactors_core::Result<()> {
        let asset = (0..61).map(|_| "A").chain(["B"]).collect::<Vec<_>>();
        let time = (1i64..=61).chain([61]).collect::<Vec<_>>();
        let open = (1..=61)
            .map(|value| value as f64)
            .chain([100.0])
            .collect::<Vec<_>>();
        let close = (2..=62)
            .map(|value| value as f64)
            .chain([110.0])
            .collect::<Vec<_>>();
        let df = df!(
            "asset" => asset,
            "time" => time,
            "open" => open,
            "close" => close,
        )?;
        let panel = PreparedPanel::new(
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
        )?;

        let out = compute_panel(
            &panel,
            Series::new("time".into(), [60i64]),
            vec!["ret".to_string()],
            None,
            &phase2_descriptors(),
        )?;
        let values = out
            .column("ret")?
            .try_f64()
            .expect("ret is f64")
            .into_no_null_iter()
            .collect::<Vec<_>>();

        assert_eq!(values[0], 61.0 / 1.0 - 1.0);
        assert!(values[1].is_nan());

        Ok(())
    }
}
