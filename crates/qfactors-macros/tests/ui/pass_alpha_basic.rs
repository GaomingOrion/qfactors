use qfactors_core::A;
use qfactors_core::alpha::{delay, open, rank, returns, sum};
use qfactors_macros::alpha;

#[alpha(name = "alpha8")]
fn alpha8_alias() -> A {
    let inner = sum(open(), 5) * sum(returns(), 5);
    -1.0 * rank(inner.clone() - delay(inner, 10))
}

fn main() {}
