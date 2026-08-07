use glass_browser::browser::policy::{BrowserPolicy, PolicyCapability};
use std::hint::black_box;
use std::time::Instant;

const ITERATIONS: usize = 1_000_000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let policy = BrowserPolicy::development(std::env::current_dir()?)?;
    let started = Instant::now();
    for _ in 0..ITERATIONS {
        black_box(policy.require(black_box(PolicyCapability::Evaluate))?);
    }
    let elapsed = started.elapsed();
    println!(
        "{{\"operation\":\"allowed_capability_check\",\"iterations\":{ITERATIONS},\"total_ns\":{},\"ns_per_check\":{}}}",
        elapsed.as_nanos(),
        elapsed.as_nanos() / ITERATIONS as u128
    );
    Ok(())
}
