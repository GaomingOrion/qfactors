use std::cmp::Ordering;

use crate::cellset::CellSet;
use crate::error::{QFactorsError, Result};
use crate::expr::Expr;
use crate::layout::{Layout, nt_to_tn, tn_to_nt};

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Cells { values: Vec<f64>, layout: Layout },
    Scalar(f64),
}

pub fn eval(expr: &Expr, cs: &CellSet) -> Result<Val> {
    match expr {
        Expr::Field(name) => cs
            .fields
            .get(name)
            .cloned()
            .map(|values| Val::Cells {
                values,
                layout: Layout::Nt,
            })
            .ok_or_else(|| QFactorsError::MissingColumn(name.clone())),
        Expr::Const(value) => Ok(Val::Scalar(*value)),
        Expr::Add(lhs, rhs) => eval_binary(lhs, rhs, cs, |a, b| a + b),
        Expr::Sub(lhs, rhs) => eval_binary(lhs, rhs, cs, |a, b| a - b),
        Expr::Mul(lhs, rhs) => eval_binary(lhs, rhs, cs, |a, b| a * b),
        Expr::Div(lhs, rhs) => eval_binary(lhs, rhs, cs, |a, b| a / b),
        Expr::Neg(inner) => match eval(inner, cs)? {
            Val::Scalar(value) => Ok(Val::Scalar(-value)),
            Val::Cells { values, layout } => Ok(Val::Cells {
                values: values.into_iter().map(|value| -value).collect(),
                layout,
            }),
        },
        Expr::Delay(inner, days) => {
            let values = to_cells(eval(inner, cs)?, Layout::Nt, cs);
            Ok(Val::Cells {
                values: delay(&values, *days, cs),
                layout: Layout::Nt,
            })
        }
        Expr::TsSum(inner, days) => {
            let values = to_cells(eval(inner, cs)?, Layout::Nt, cs);
            Ok(Val::Cells {
                values: ts_sum(&values, *days, cs),
                layout: Layout::Nt,
            })
        }
        Expr::Rank(inner) => {
            let values = to_cells(eval(inner, cs)?, Layout::Tn, cs);
            Ok(Val::Cells {
                values: rank(&values, cs),
                layout: Layout::Tn,
            })
        }
    }
}

pub fn to_cells(value: Val, want: Layout, cs: &CellSet) -> Vec<f64> {
    match value {
        Val::Scalar(value) => vec![value; cs.n_cells],
        Val::Cells { values, layout } if layout == want => values,
        Val::Cells {
            values,
            layout: Layout::Nt,
        } => nt_to_tn(&values, cs),
        Val::Cells {
            values,
            layout: Layout::Tn,
        } => tn_to_nt(&values, cs),
    }
}

fn eval_binary(lhs: &Expr, rhs: &Expr, cs: &CellSet, op: impl Fn(f64, f64) -> f64) -> Result<Val> {
    let lhs = eval(lhs, cs)?;
    let rhs = eval(rhs, cs)?;
    match (&lhs, &rhs) {
        (Val::Scalar(lhs), Val::Scalar(rhs)) => Ok(Val::Scalar(op(*lhs, *rhs))),
        _ => {
            let layout = match (&lhs, &rhs) {
                (Val::Cells { layout, .. }, _) | (_, Val::Cells { layout, .. }) => *layout,
                (Val::Scalar(_), Val::Scalar(_)) => unreachable!("handled above"),
            };
            let lhs = to_cells(lhs, layout, cs);
            let rhs = to_cells(rhs, layout, cs);
            Ok(Val::Cells {
                values: lhs
                    .into_iter()
                    .zip(rhs)
                    .map(|(lhs, rhs)| op(lhs, rhs))
                    .collect(),
                layout,
            })
        }
    }
}

fn delay(values: &[f64], days: usize, cs: &CellSet) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    for block in &cs.sym_blocks {
        for local_idx in days..block.len() {
            out[block.start + local_idx] = values[block.start + local_idx - days];
        }
    }
    out
}

fn ts_sum(values: &[f64], days: usize, cs: &CellSet) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    if days == 0 {
        return out;
    }

    for block in &cs.sym_blocks {
        for local_idx in days - 1..block.len() {
            let start = block.start + local_idx + 1 - days;
            let end = block.start + local_idx;
            let mut total = 0.0;
            let mut valid = true;
            for value in &values[start..=end] {
                if value.is_nan() {
                    valid = false;
                    break;
                }
                total += value;
            }
            if valid {
                out[end] = total;
            }
        }
    }
    out
}

fn rank(values: &[f64], cs: &CellSet) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];

    for block in &cs.time_blocks {
        let mut present = block
            .clone()
            .filter_map(|idx| {
                let value = values[idx];
                (!value.is_nan()).then_some((idx, value))
            })
            .collect::<Vec<_>>();

        present.sort_by(|(_, lhs), (_, rhs)| lhs.partial_cmp(rhs).unwrap_or(Ordering::Equal));
        let count = present.len() as f64;
        let mut start = 0usize;
        while start < present.len() {
            let mut end = start + 1;
            while end < present.len() && present[end].1 == present[start].1 {
                end += 1;
            }

            let rank_avg = (start + 1 + end) as f64 / 2.0;
            let pct = rank_avg / count;
            for (idx, _) in &present[start..end] {
                out[*idx] = pct;
            }
            start = end;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ops::Range;

    use polars::prelude::*;

    use super::*;

    fn test_cellset(
        values: Vec<f64>,
        sym_blocks: Vec<Range<usize>>,
        time_blocks: Vec<Range<usize>>,
    ) -> CellSet {
        let n_cells = values.len();
        CellSet {
            n_cells,
            sym_blocks,
            time_blocks,
            tn_order: (0..n_cells).collect(),
            fields: HashMap::from([("x".to_string(), values)]),
            symbols_tn: Column::new("asset".into(), vec!["A"; n_cells]),
            times_tn: Column::new("time".into(), (0..n_cells as i64).collect::<Vec<_>>()),
            time_block_by_value: HashMap::new(),
        }
    }

    fn cells(value: Val, cs: &CellSet) -> Vec<f64> {
        to_cells(value, Layout::Nt, cs)
    }

    fn one_block(range: Range<usize>) -> Vec<Range<usize>> {
        std::iter::once(range).collect()
    }

    #[test]
    fn delay_shifts_with_nan_prefix() -> Result<()> {
        let cs = test_cellset(
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            one_block(0..5),
            vec![0..1, 1..2, 2..3, 3..4, 4..5],
        );

        let out = cells(
            eval(&Expr::Delay(Box::new(Expr::Field("x".to_string())), 2), &cs)?,
            &cs,
        );

        assert!(out[0].is_nan());
        assert!(out[1].is_nan());
        assert_eq!(&out[2..], &[1.0, 2.0, 3.0]);
        Ok(())
    }

    #[test]
    fn ts_sum_requires_full_non_nan_window() -> Result<()> {
        let cs = test_cellset(
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            one_block(0..5),
            vec![0..1, 1..2, 2..3, 3..4, 4..5],
        );

        let out = cells(
            eval(&Expr::TsSum(Box::new(Expr::Field("x".to_string())), 3), &cs)?,
            &cs,
        );

        assert!(out[0].is_nan());
        assert!(out[1].is_nan());
        assert_eq!(&out[2..], &[6.0, 9.0, 12.0]);

        let nan_cs = test_cellset(
            vec![1.0, 2.0, f64::NAN, 4.0, 5.0],
            one_block(0..5),
            vec![0..1, 1..2, 2..3, 3..4, 4..5],
        );
        let nan_out = cells(
            eval(
                &Expr::TsSum(Box::new(Expr::Field("x".to_string())), 3),
                &nan_cs,
            )?,
            &nan_cs,
        );
        assert!(nan_out.iter().all(|value| value.is_nan()));
        Ok(())
    }

    #[test]
    fn rank_averages_ties_and_skips_nan() -> Result<()> {
        let tie_cs = test_cellset(
            vec![10.0, 20.0, 20.0, 30.0],
            vec![0..1, 1..2, 2..3, 3..4],
            one_block(0..4),
        );
        let tie_out = to_cells(
            eval(&Expr::Rank(Box::new(Expr::Field("x".to_string()))), &tie_cs)?,
            Layout::Tn,
            &tie_cs,
        );
        assert_eq!(tie_out, [0.25, 0.625, 0.625, 1.0]);

        let nan_cs = test_cellset(
            vec![10.0, f64::NAN, 20.0, 30.0],
            vec![0..1, 1..2, 2..3, 3..4],
            one_block(0..4),
        );
        let nan_out = to_cells(
            eval(&Expr::Rank(Box::new(Expr::Field("x".to_string()))), &nan_cs)?,
            Layout::Tn,
            &nan_cs,
        );
        assert_eq!(nan_out[0], 1.0 / 3.0);
        assert!(nan_out[1].is_nan());
        assert_eq!(nan_out[2], 2.0 / 3.0);
        assert_eq!(nan_out[3], 1.0);
        Ok(())
    }

    #[test]
    fn binary_ops_preserve_scalar_when_possible() -> Result<()> {
        let cs = test_cellset(Vec::new(), Vec::new(), Vec::new());
        let out = eval(
            &Expr::Sub(Box::new(Expr::Const(3.0)), Box::new(Expr::Const(1.5))),
            &cs,
        )?;

        assert_eq!(out, Val::Scalar(1.5));
        Ok(())
    }
}
