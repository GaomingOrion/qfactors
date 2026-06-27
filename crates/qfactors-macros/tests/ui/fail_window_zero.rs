use qfactors_macros::factor;

#[factor(window = 0)]
fn bad(values: &[f64]) -> f64 {
    values[0]
}

fn main() {}
