use qfactors_macros::factor;

pub fn ensure_linked() {}

#[factor(window = 60)]
pub fn ret(open: &[f64], close: &[f64]) -> f64 {
    close[close.len() - 1] / open[0] - 1.0
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use polars::prelude::*;
    use qfactors_core::{NullPolicy, PreparePanelOptions, PreparedPanel, Result, compute_panel};

    use super::*;

    #[factor(windows = [2, 3])]
    fn delta(close: &[f64]) -> f64 {
        close[close.len() - 1] - close[0]
    }

    #[factor(window = 2, outputs = ["first", "last"])]
    fn bounds(close: &[f64]) -> (f64, f64) {
        (close[0], close[close.len() - 1])
    }

    #[factor(window = 2)]
    fn checked_delta(close: &[f64]) -> Result<f64> {
        Ok(close[close.len() - 1] - close[0])
    }

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

    #[test]
    fn macro_generated_factors_support_windows_outputs_and_result() -> qfactors_core::Result<()> {
        let df = df!(
            "asset" => ["A", "A", "A"],
            "time" => [1i64, 2, 3],
            "open" => [10.0, 11.0, 12.0],
            "close" => [20.0, 23.0, 27.0],
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
            Series::new("time".into(), [3i64]),
            vec![
                "delta_2".to_string(),
                "delta_3".to_string(),
                "bounds".to_string(),
                "checked_delta".to_string(),
            ],
            None,
        )?;

        assert_eq!(
            out.column("delta_2")?
                .try_f64()
                .expect("delta_2 is f64")
                .get(0),
            Some(4.0)
        );
        assert_eq!(
            out.column("delta_3")?
                .try_f64()
                .expect("delta_3 is f64")
                .get(0),
            Some(7.0)
        );
        assert_eq!(
            out.column("bounds.first")?
                .try_f64()
                .expect("bounds.first is f64")
                .get(0),
            Some(23.0)
        );
        assert_eq!(
            out.column("bounds.last")?
                .try_f64()
                .expect("bounds.last is f64")
                .get(0),
            Some(27.0)
        );
        assert_eq!(
            out.column("checked_delta")?
                .try_f64()
                .expect("checked_delta is f64")
                .get(0),
            Some(4.0)
        );

        Ok(())
    }
}
