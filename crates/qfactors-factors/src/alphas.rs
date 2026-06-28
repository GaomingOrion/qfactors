pub use qfactors_core::A;
pub use qfactors_core::alpha::{close, delay, field, open, rank, returns, sum};
use qfactors_macros::alpha;

#[alpha]
pub fn alpha8() -> A {
    let inner = sum(open(), 5) * sum(returns(), 5);
    -1.0 * rank(inner.clone() - delay(inner, 10))
}
