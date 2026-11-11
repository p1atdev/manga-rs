//! Test-only paired benchmark support. Run ignored benchmarks in release mode.
use std::time::{Duration, Instant};

const BLOCKS: usize = 100;
const T_95_99_DOF: f64 = 1.984;

fn measure(operation: &mut impl FnMut()) -> f64 {
    let start = Instant::now();
    let mut count = 0u64;
    while start.elapsed() < Duration::from_millis(50) {
        operation();
        count += 1;
    }
    start.elapsed().as_secs_f64() * 1000.0 / count as f64
}

// The Student-t interval is descriptive; scheduling noise can
// still distort these short measurements on a shared machine.
fn summarize(samples: &mut [f64]) -> (f64, f64, f64, f64, f64) {
    let mean = samples.iter().sum::<f64>() / BLOCKS as f64;
    let variance = samples.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (BLOCKS - 1) as f64;
    let margin = T_95_99_DOF * (variance / BLOCKS as f64).sqrt();
    samples.sort_by(f64::total_cmp);
    let median = (samples[BLOCKS / 2 - 1] + samples[BLOCKS / 2]) / 2.0;
    (mean, margin, median, samples[0], samples[BLOCKS - 1])
}

pub(crate) fn paired(label: &str, mut old: impl FnMut(), mut new: impl FnMut()) {
    old();
    new();
    let mut old_samples = Vec::with_capacity(BLOCKS);
    let mut new_samples = Vec::with_capacity(BLOCKS);
    let mut improvements = Vec::with_capacity(BLOCKS);
    for _ in 0..BLOCKS {
        let a1 = measure(&mut old);
        let b1 = measure(&mut new);
        let b2 = measure(&mut new);
        let a2 = measure(&mut old);
        let a = (a1 + a2) / 2.0;
        let b = (b1 + b2) / 2.0;
        old_samples.push(a);
        new_samples.push(b);
        improvements.push(100.0 * (1.0 - b / a));
    }
    let (a, da, ma, _, _) = summarize(&mut old_samples);
    let (b, db, mb, _, _) = summarize(&mut new_samples);
    let (diff, margin, median, min, max) = summarize(&mut improvements);
    println!(
        "{label}: old_ms={a:.6} ci95=[{:.6},{:.6}] median={ma:.6}; new_ms={b:.6} ci95=[{:.6},{:.6}] median={mb:.6}; paired_improvement_mean={diff:.3}% ci95=[{:.3}%,{:.3}%] median={median:.3}% range=[{min:.3}%,{max:.3}%] blocks={BLOCKS} min_interval=50ms",
        a - da,
        a + da,
        b - db,
        b + db,
        diff - margin,
        diff + margin
    );
}
