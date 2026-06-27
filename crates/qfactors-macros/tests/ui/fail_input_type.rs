use qfactors_macros::factor;

#[factor(window = 2)]
fn bad(values: &[i32]) -> f64 {
    values[0] as f64
}

fn main() {}
