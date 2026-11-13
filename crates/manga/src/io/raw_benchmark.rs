//! Reproduce with:
//! cargo test --release -p manga jpeg_file_abba_benchmark -- --ignored --nocapture --test-threads=1
//!
//! This benchmark measures encoding and cached filesystem writes, including
//! file creation and buffer flush. It excludes image decoding and fsync.
use super::write_image;
use image::{DynamicImage, ImageFormat};
use std::{
    fs::{self, File},
    hint::black_box,
    io::{self, Cursor, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const BLOCKS: usize = 100;
const T_95_99_DOF: f64 = 1.984;

struct OwnedDir(Option<PathBuf>);
impl OwnedDir {
    fn new() -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "manga-jpeg-io-bench-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self(Some(path)))
    }
    fn path(&self) -> &Path {
        self.0.as_deref().expect("directory has not been removed")
    }
    fn finish(mut self) -> io::Result<()> {
        fs::remove_dir_all(self.path())?;
        self.0 = None;
        Ok(())
    }
}
impl Drop for OwnedDir {
    fn drop(&mut self) {
        if let Some(path) = &self.0
            && let Err(error) = fs::remove_dir_all(path)
        {
            eprintln!("failed to clean up JPEG benchmark directory: {error}");
        }
    }
}

struct CountingFile {
    file: File,
    writes: usize,
}
impl Write for CountingFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.writes += 1;
        self.file.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
impl Seek for CountingFile {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.file.seek(position)
    }
}
fn count_writes(image: &DynamicImage, path: &Path, buffered: bool) -> anyhow::Result<usize> {
    let mut file = CountingFile {
        file: File::create(path)?,
        writes: 0,
    };
    if buffered {
        write_image(&mut file, image, ImageFormat::Jpeg)?;
    } else {
        image.write_to(&mut file, ImageFormat::Jpeg)?;
    }
    Ok(file.writes)
}
fn write_once(image: &DynamicImage, path: &Path, buffered: bool) -> anyhow::Result<()> {
    let mut file = File::create(path)?;
    if buffered {
        write_image(&mut file, image, ImageFormat::Jpeg)?;
    } else {
        image.write_to(&mut file, ImageFormat::Jpeg)?;
    }
    Ok(())
}
fn measure(image: &DynamicImage, path: &Path, buffered: bool) -> f64 {
    let start = Instant::now();
    let mut count = 0u64;
    while start.elapsed() < Duration::from_millis(50) {
        write_once(black_box(image), path, buffered).expect("benchmark write failed");
        count += 1;
    }
    start.elapsed().as_secs_f64() * 1000.0 / count as f64
}
// The Student-t interval is descriptive; shared-host noise may distort it.
fn summary(samples: &mut [f64]) -> (f64, f64, f64, f64, f64) {
    let mean = samples.iter().sum::<f64>() / BLOCKS as f64;
    let variance = samples.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (BLOCKS - 1) as f64;
    let margin = T_95_99_DOF * (variance / BLOCKS as f64).sqrt();
    samples.sort_by(f64::total_cmp);
    (
        mean,
        margin,
        (samples[BLOCKS / 2 - 1] + samples[BLOCKS / 2]) / 2.0,
        samples[0],
        samples[BLOCKS - 1],
    )
}
fn compare(image: &DynamicImage, root: &Path, label: &str) -> anyhow::Result<()> {
    let old_path = root.join("direct.jpg");
    let new_path = root.join("buffered.jpg");
    let old_calls = count_writes(image, &old_path, false)?;
    let new_calls = count_writes(image, &new_path, true)?;
    let expected = fs::read(&old_path)?;
    assert_eq!(fs::read(&new_path)?, expected, "initial output differs");
    println!(
        "{label}: dimensions={}x{}, bytes={}, direct_write_calls={old_calls}, buffered_write_calls={new_calls}, byte_identical=true",
        image.width(),
        image.height(),
        expected.len()
    );
    let mut old_samples = Vec::with_capacity(BLOCKS);
    let mut new_samples = Vec::with_capacity(BLOCKS);
    let mut improvements = Vec::with_capacity(BLOCKS);
    for _ in 0..BLOCKS {
        let a1 = measure(image, &old_path, false);
        let b1 = measure(image, &new_path, true);
        let b2 = measure(image, &new_path, true);
        let a2 = measure(image, &old_path, false);
        let a = (a1 + a2) / 2.0;
        let b = (b1 + b2) / 2.0;
        old_samples.push(a);
        new_samples.push(b);
        improvements.push(100.0 * (1.0 - b / a));
    }
    assert_eq!(
        fs::read(&old_path)?,
        expected,
        "final direct output differs"
    );
    assert_eq!(
        fs::read(&new_path)?,
        expected,
        "final buffered output differs"
    );
    let (a, da, ma, _, _) = summary(&mut old_samples);
    let (b, db, mb, _, _) = summary(&mut new_samples);
    let (diff, margin, median, min, max) = summary(&mut improvements);
    println!(
        "{label}: old_ms={a:.4} ci95=[{:.4},{:.4}] median={ma:.4}; new_ms={b:.4} ci95=[{:.4},{:.4}] median={mb:.4}; paired_improvement_mean={diff:.3}% ci95=[{:.3}%,{:.3}%] median={median:.3}% range=[{min:.3}%,{max:.3}%] blocks={BLOCKS} min_interval=50ms",
        a - da,
        a + da,
        b - db,
        b + db,
        diff - margin,
        diff + margin
    );
    Ok(())
}

#[test]
#[ignore = "manual JPEG filesystem benchmark: 100 paired blocks per input"]
fn jpeg_file_abba_benchmark() -> anyhow::Result<()> {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/assets/giga-original.jpg"
    );
    let rgb = image::open(fixture)?;
    let rgba = DynamicImage::ImageRgba8(rgb.to_rgba8());
    let mut encoded = Cursor::new(Vec::new());
    rgba.write_to(&mut encoded, ImageFormat::Png)?;
    let png_rgba = image::load_from_memory(encoded.get_ref())?;
    assert_eq!(png_rgba.color(), image::ColorType::Rgba8);
    let dir = OwnedDir::new()?;
    println!(
        "JPEG benchmark: cached filesystem, image decode excluded, file creation and flush included, fsync excluded"
    );
    compare(&rgb, dir.path(), "fixture RGB")?;
    compare(&png_rgba, dir.path(), "synthetic PNG RGBA")?;
    dir.finish()?;
    Ok(())
}
