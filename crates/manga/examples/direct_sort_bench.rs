use manga::{
    solver::ImageSolver,
    viewer::giga::{direct_sort::sort_jpeg, solver::Solver},
};
use std::{env, fs, hint::black_box, io::Cursor, time::Instant};
fn millis(mut f: impl FnMut()) -> f64 {
    let t = Instant::now();
    f();
    t.elapsed().as_secs_f64() * 1e3
}
fn main() {
    let path = env::args().nth(1).unwrap();
    let data = fs::read(path).unwrap();
    let direct = || {
        black_box(sort_jpeg(black_box(&data)).unwrap());
    };
    let standard = || {
        let image = Solver::new()
            .solve_from_bytes(black_box(data.clone()))
            .unwrap();
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        black_box(out.into_inner());
    };
    direct();
    standard();
    let mut diffs = Vec::new();
    let mut olds = Vec::new();
    let mut news = Vec::new();
    for _ in 0..100 {
        let a1 = millis(standard);
        let b1 = millis(direct);
        let b2 = millis(direct);
        let a2 = millis(standard);
        let a = (a1 + a2) / 2.;
        let b = (b1 + b2) / 2.;
        olds.push(a);
        news.push(b);
        diffs.push(100. * (1. - b / a));
    }
    let mean = |v: &Vec<f64>| v.iter().sum::<f64>() / v.len() as f64;
    let m = mean(&diffs);
    let margin = 1.984 * (diffs.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / 99. / 100.).sqrt();
    println!(
        "old_ms={:.4} direct_ms={:.4} paired_improvement={:.3}% ci95=[{:.3}%,{:.3}%] blocks=100 ABBA",
        mean(&olds),
        mean(&news),
        m,
        m - margin,
        m + margin
    );
}
