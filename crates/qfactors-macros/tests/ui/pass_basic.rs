use qfactors_macros::factor;

#[factor(window = 2)]
fn ret(open: &[f64], close: &[f64]) -> f64 {
    close[close.len() - 1] / open[0] - 1.0
}

fn main() {}
