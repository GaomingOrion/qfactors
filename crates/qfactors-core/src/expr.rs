use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Field(String),
    Const(f64),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Delay(Box<Expr>, usize),
    TsSum(Box<Expr>, usize),
    Rank(Box<Expr>),
}

pub fn collect_fields(expr: &Expr, out: &mut BTreeSet<String>) {
    match expr {
        Expr::Field(name) => {
            out.insert(name.clone());
        }
        Expr::Const(_) => {}
        Expr::Add(lhs, rhs) | Expr::Sub(lhs, rhs) | Expr::Mul(lhs, rhs) | Expr::Div(lhs, rhs) => {
            collect_fields(lhs, out);
            collect_fields(rhs, out);
        }
        Expr::Neg(inner) | Expr::Delay(inner, _) | Expr::TsSum(inner, _) | Expr::Rank(inner) => {
            collect_fields(inner, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_fields_deduplicates_and_sorts() {
        let expr = Expr::Add(
            Box::new(Expr::Field("close".to_string())),
            Box::new(Expr::Delay(Box::new(Expr::Field("open".to_string())), 1)),
        );
        let mut fields = BTreeSet::new();

        collect_fields(&expr, &mut fields);

        assert_eq!(
            fields.into_iter().collect::<Vec<_>>(),
            ["close".to_string(), "open".to_string()]
        );
    }
}
