use qfactors_macros::factor;

#[factor(window = 2)]
fn bad(values: &[f64]) -> u32 {
    values.len() as u32
}

fn main() {}
