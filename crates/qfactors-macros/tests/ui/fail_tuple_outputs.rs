use qfactors_macros::factor;

#[factor(window = 2, outputs = ["first"])]
fn bad(values: &[f64]) -> (f64, f64) {
    (values[0], values[values.len() - 1])
}

fn main() {}
