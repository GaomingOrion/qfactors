use std::cmp::Ordering;

use crate::error::{QFactorsError, Result};
use crate::expr::Expr;
use crate::grid::GridCtx;

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Grid(Vec<f64>),
    Scalar(f64),
}

pub fn eval(expr: &Expr, ctx: &GridCtx) -> Result<Val> {
    match expr {
        Expr::Field(name) => ctx
            .fields
            .get(name)
            .cloned()
            .map(Val::Grid)
            .ok_or_else(|| QFactorsError::MissingColumn(name.clone())),
        Expr::Const(value) => Ok(Val::Scalar(*value)),
        Expr::Add(lhs, rhs) => eval_binary(lhs, rhs, ctx, |a, b| a + b),
        Expr::Sub(lhs, rhs) => eval_binary(lhs, rhs, ctx, |a, b| a - b),
        Expr::Mul(lhs, rhs) => eval_binary(lhs, rhs, ctx, |a, b| a * b),
        Expr::Div(lhs, rhs) => eval_binary(lhs, rhs, ctx, |a, b| a / b),
        Expr::Neg(inner) => match eval(inner, ctx)? {
            Val::Scalar(value) => Ok(Val::Scalar(-value)),
            Val::Grid(values) => Ok(Val::Grid(values.into_iter().map(|value| -value).collect())),
        },
        Expr::Delay(inner, days) => {
            let values = to_grid(eval(inner, ctx)?, ctx);
            Ok(Val::Grid(delay(&values, *days, ctx)))
        }
        Expr::TsSum(inner, days) => {
            let values = to_grid(eval(inner, ctx)?, ctx);
            Ok(Val::Grid(ts_sum(&values, *days, ctx)))
        }
        Expr::Rank(inner) => {
            let values = to_grid(eval(inner, ctx)?, ctx);
            Ok(Val::Grid(rank(&values, ctx)))
        }
    }
}

pub fn to_grid(value: Val, ctx: &GridCtx) -> Vec<f64> {
    match value {
        Val::Grid(values) => values,
        Val::Scalar(value) => vec![value; ctx.n_symbols * ctx.n_times],
    }
}

fn eval_binary(lhs: &Expr, rhs: &Expr, ctx: &GridCtx, op: impl Fn(f64, f64) -> f64) -> Result<Val> {
    match (eval(lhs, ctx)?, eval(rhs, ctx)?) {
        (Val::Scalar(lhs), Val::Scalar(rhs)) => Ok(Val::Scalar(op(lhs, rhs))),
        (lhs, rhs) => {
            let lhs = to_grid(lhs, ctx);
            let rhs = to_grid(rhs, ctx);
            Ok(Val::Grid(
                lhs.into_iter()
                    .zip(rhs)
                    .map(|(lhs, rhs)| op(lhs, rhs))
                    .collect(),
            ))
        }
    }
}

fn delay(values: &[f64], days: usize, ctx: &GridCtx) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    for symbol in 0..ctx.n_symbols {
        let offset = symbol * ctx.n_times;
        for time in days..ctx.n_times {
            out[offset + time] = values[offset + time - days];
        }
    }
    out
}

fn ts_sum(values: &[f64], days: usize, ctx: &GridCtx) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    if days == 0 {
        return out;
    }

    for symbol in 0..ctx.n_symbols {
        let offset = symbol * ctx.n_times;
        for time in days - 1..ctx.n_times {
            let mut total = 0.0;
            let mut valid = true;
            for window_time in time + 1 - days..=time {
                let value = values[offset + window_time];
                if value.is_nan() {
                    valid = false;
                    break;
                }
                total += value;
            }
            if valid {
                out[offset + time] = total;
            }
        }
    }
    out
}

fn rank(values: &[f64], ctx: &GridCtx) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];

    for time in 0..ctx.n_times {
        let mut present = (0..ctx.n_symbols)
            .filter_map(|symbol| {
                let value = values[symbol * ctx.n_times + time];
                (!value.is_nan()).then_some((symbol, value))
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
            for (symbol, _) in &present[start..end] {
                out[*symbol * ctx.n_times + time] = pct;
            }
            start = end;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn test_ctx(values: Vec<f64>, n_symbols: usize, n_times: usize) -> GridCtx {
        GridCtx {
            n_symbols,
            n_times,
            fields: HashMap::from([("x".to_string(), values)]),
            presence: vec![true; n_symbols * n_times],
        }
    }

    fn grid(value: Val, ctx: &GridCtx) -> Vec<f64> {
        to_grid(value, ctx)
    }

    #[test]
    fn delay_shifts_with_nan_prefix() -> Result<()> {
        let ctx = test_ctx(vec![1.0, 2.0, 3.0, 4.0, 5.0], 1, 5);

        let out = grid(
            eval(
                &Expr::Delay(Box::new(Expr::Field("x".to_string())), 2),
                &ctx,
            )?,
            &ctx,
        );

        assert!(out[0].is_nan());
        assert!(out[1].is_nan());
        assert_eq!(&out[2..], &[1.0, 2.0, 3.0]);
        Ok(())
    }

    #[test]
    fn ts_sum_requires_full_non_nan_window() -> Result<()> {
        let ctx = test_ctx(vec![1.0, 2.0, 3.0, 4.0, 5.0], 1, 5);

        let out = grid(
            eval(
                &Expr::TsSum(Box::new(Expr::Field("x".to_string())), 3),
                &ctx,
            )?,
            &ctx,
        );

        assert!(out[0].is_nan());
        assert!(out[1].is_nan());
        assert_eq!(&out[2..], &[6.0, 9.0, 12.0]);

        let nan_ctx = test_ctx(vec![1.0, 2.0, f64::NAN, 4.0, 5.0], 1, 5);
        let nan_out = grid(
            eval(
                &Expr::TsSum(Box::new(Expr::Field("x".to_string())), 3),
                &nan_ctx,
            )?,
            &nan_ctx,
        );
        assert!(nan_out.iter().all(|value| value.is_nan()));
        Ok(())
    }

    #[test]
    fn rank_averages_ties_and_skips_nan() -> Result<()> {
        let tie_ctx = test_ctx(vec![10.0, 20.0, 20.0, 30.0], 4, 1);
        let tie_out = grid(
            eval(
                &Expr::Rank(Box::new(Expr::Field("x".to_string()))),
                &tie_ctx,
            )?,
            &tie_ctx,
        );
        assert_eq!(tie_out, [0.25, 0.625, 0.625, 1.0]);

        let nan_ctx = test_ctx(vec![10.0, f64::NAN, 20.0, 30.0], 4, 1);
        let nan_out = grid(
            eval(
                &Expr::Rank(Box::new(Expr::Field("x".to_string()))),
                &nan_ctx,
            )?,
            &nan_ctx,
        );
        assert_eq!(nan_out[0], 1.0 / 3.0);
        assert!(nan_out[1].is_nan());
        assert_eq!(nan_out[2], 2.0 / 3.0);
        assert_eq!(nan_out[3], 1.0);
        Ok(())
    }

    #[test]
    fn binary_ops_preserve_scalar_when_possible() -> Result<()> {
        let ctx = test_ctx(Vec::new(), 0, 0);
        let out = eval(
            &Expr::Sub(Box::new(Expr::Const(3.0)), Box::new(Expr::Const(1.5))),
            &ctx,
        )?;

        assert_eq!(out, Val::Scalar(1.5));
        Ok(())
    }
}
