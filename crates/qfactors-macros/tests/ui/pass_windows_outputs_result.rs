use qfactors_macros::factor;

#[factor(windows = [2, 3], outputs = ["change", "volume"])]
fn stats(close: &[f64], volume: &[u32], timestamp: &[i64]) -> qfactors_core::Result<(f64, f64)> {
    let change = close[close.len() - 1] - close[0];
    let last_volume = volume[volume.len() - 1] as f64;
    let _last_timestamp = timestamp[timestamp.len() - 1];
    Ok((change, last_volume))
}

fn main() {}
