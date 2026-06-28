use std::ops::{Add, Div, Mul, Neg, Sub};

use crate::expr::Expr;

#[derive(Debug, Clone, PartialEq)]
pub struct A(Expr);

impl A {
    pub fn into_expr(self) -> Expr {
        self.0
    }
}

pub fn field(name: &str) -> A {
    A(Expr::Field(name.to_string()))
}

pub fn open() -> A {
    field("open")
}

pub fn close() -> A {
    field("close")
}

pub fn returns() -> A {
    close() / delay(close(), 1) - 1.0
}

pub fn rank(x: A) -> A {
    A(Expr::Rank(Box::new(x.into_expr())))
}

pub fn delay(x: A, d: usize) -> A {
    A(Expr::Delay(Box::new(x.into_expr()), d))
}

pub fn sum(x: A, d: usize) -> A {
    A(Expr::TsSum(Box::new(x.into_expr()), d))
}

impl Add for A {
    type Output = A;

    fn add(self, rhs: Self) -> Self::Output {
        A(Expr::Add(
            Box::new(self.into_expr()),
            Box::new(rhs.into_expr()),
        ))
    }
}

impl Sub for A {
    type Output = A;

    fn sub(self, rhs: Self) -> Self::Output {
        A(Expr::Sub(
            Box::new(self.into_expr()),
            Box::new(rhs.into_expr()),
        ))
    }
}

impl Mul for A {
    type Output = A;

    fn mul(self, rhs: Self) -> Self::Output {
        A(Expr::Mul(
            Box::new(self.into_expr()),
            Box::new(rhs.into_expr()),
        ))
    }
}

impl Div for A {
    type Output = A;

    fn div(self, rhs: Self) -> Self::Output {
        A(Expr::Div(
            Box::new(self.into_expr()),
            Box::new(rhs.into_expr()),
        ))
    }
}

impl Sub<f64> for A {
    type Output = A;

    fn sub(self, rhs: f64) -> Self::Output {
        A(Expr::Sub(
            Box::new(self.into_expr()),
            Box::new(Expr::Const(rhs)),
        ))
    }
}

impl Mul<f64> for A {
    type Output = A;

    fn mul(self, rhs: f64) -> Self::Output {
        A(Expr::Mul(
            Box::new(self.into_expr()),
            Box::new(Expr::Const(rhs)),
        ))
    }
}

impl Mul<A> for f64 {
    type Output = A;

    fn mul(self, rhs: A) -> Self::Output {
        A(Expr::Mul(
            Box::new(Expr::Const(self)),
            Box::new(rhs.into_expr()),
        ))
    }
}

impl Neg for A {
    type Output = A;

    fn neg(self) -> Self::Output {
        A(Expr::Neg(Box::new(self.into_expr())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_expands_to_close_div_delay_minus_one() {
        assert_eq!(
            returns().into_expr(),
            Expr::Sub(
                Box::new(Expr::Div(
                    Box::new(Expr::Field("close".to_string())),
                    Box::new(Expr::Delay(Box::new(Expr::Field("close".to_string())), 1)),
                )),
                Box::new(Expr::Const(1.0)),
            )
        );
    }

    #[test]
    fn alpha8_expression_can_be_built_with_clone() {
        let inner = sum(open(), 5) * sum(returns(), 5);
        let expr = (-1.0 * rank(inner.clone() - delay(inner, 10))).into_expr();

        assert!(matches!(expr, Expr::Mul(_, _)));
    }
}
