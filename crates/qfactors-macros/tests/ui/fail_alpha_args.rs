use qfactors_macros::alpha;

#[alpha]
fn bad(value: f64) -> qfactors_core::A {
    let _ = value;
    qfactors_core::alpha::open()
}

fn main() {}
