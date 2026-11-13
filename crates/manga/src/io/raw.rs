use anyhow::{Ok, Result};
use std::{
    io::{BufWriter, Seek, Write},
    path::{Path, PathBuf},
};

use super::EpisodeWriter;

#[derive(Debug, Clone)]
pub struct RawWriter {
    image_format: image::ImageFormat,
    save_path: PathBuf,
}

impl RawWriter {
    pub fn new<P: AsRef<Path>>(image_format: image::ImageFormat, save_path: &P) -> Self {
        RawWriter {
            image_format,
            save_path: save_path.as_ref().to_path_buf(),
        }
    }

    pub fn default<P: AsRef<Path>>(save_path: &P) -> Self {
        RawWriter {
            image_format: image::ImageFormat::Png,
            save_path: save_path.as_ref().to_path_buf(),
        }
    }
}

impl EpisodeWriter for RawWriter {
    fn save_path(&self) -> &Path {
        &self.save_path
    }

    async fn prepare(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.save_path).await?;

        Ok(())
    }

    async fn write_page(&self, page: usize, image: image::DynamicImage) -> Result<()> {
        let image_name = format!("{page:04}.{}", self.image_format.extensions_str()[0]);
        let save_path = self.save_path.join(image_name);
        let image_format = self.image_format;

        tokio::task::spawn_blocking(move || {
            let mut file = std::fs::File::create(save_path)?;
            write_image(&mut file, &image, image_format)
        })
        .await?
    }
}

// JPEG emits very small writes; other encoders keep their existing I/O path.
fn write_image<W: Write + Seek>(
    writer: &mut W,
    image: &image::DynamicImage,
    format: image::ImageFormat,
) -> Result<()> {
    if format == image::ImageFormat::Jpeg {
        let mut writer = BufWriter::new(writer);
        image.write_to(&mut writer, format)?;
        writer.flush()?;
    } else {
        image.write_to(writer, format)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
    use std::io::{self, Cursor, Seek, SeekFrom, Write};

    #[test]
    fn jpeg_output_matches_direct_encoding_byte_for_byte() {
        let rgb = DynamicImage::ImageRgb8(RgbImage::from_fn(31, 29, |x, y| {
            Rgb([x as u8, y as u8, (x ^ y) as u8])
        }));
        let rgba = DynamicImage::ImageRgba8(RgbaImage::from_fn(31, 29, |x, y| {
            Rgba([x as u8, y as u8, (x ^ y) as u8, ((x + y) * 5) as u8])
        }));
        for image in [rgb, rgba] {
            let mut expected = Cursor::new(Vec::new());
            image.write_to(&mut expected, ImageFormat::Jpeg).unwrap();
            let mut actual = Cursor::new(Vec::new());
            write_image(&mut actual, &image, ImageFormat::Jpeg).unwrap();
            assert_eq!(actual.into_inner(), expected.into_inner());
        }
    }

    #[tokio::test]
    async fn raw_writer_produces_expected_jpeg_file() {
        struct TestDir(std::path::PathBuf);
        impl Drop for TestDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = TestDir(
            std::env::temp_dir().join(format!("manga-raw-jpeg-{}-{unique}", std::process::id())),
        );
        let image = DynamicImage::ImageRgb8(RgbImage::from_fn(31, 29, |x, y| {
            Rgb([x as u8, y as u8, (x ^ y) as u8])
        }));
        let mut expected = Cursor::new(Vec::new());
        image.write_to(&mut expected, ImageFormat::Jpeg).unwrap();
        let writer = RawWriter::new(ImageFormat::Jpeg, &path.0);
        writer.prepare().await.unwrap();
        writer.write_page(7, image).await.unwrap();
        assert_eq!(
            std::fs::read(path.0.join("0007.jpg")).unwrap(),
            expected.into_inner()
        );
    }

    struct FailingWriter;
    impl Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("synthetic flush failure"))
        }
        fn flush(&mut self) -> io::Result<()> {
            std::result::Result::Ok(())
        }
    }
    impl Seek for FailingWriter {
        fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
            std::result::Result::Ok(0)
        }
    }

    #[test]
    fn jpeg_reports_errors_when_flushing_the_buffer() {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 2, Rgb([1, 2, 3])));
        let mut encoded = Cursor::new(Vec::new());
        image.write_to(&mut encoded, ImageFormat::Jpeg).unwrap();
        assert!(encoded.get_ref().len() < BufWriter::new(Cursor::new(Vec::new())).capacity());
        let error = write_image(&mut FailingWriter, &image, ImageFormat::Jpeg).unwrap_err();
        assert!(format!("{error:#}").contains("synthetic flush failure"));
    }

    struct FlushFail;
    impl Write for FlushFail {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            std::result::Result::Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("synthetic underlying flush failure"))
        }
    }
    impl Seek for FlushFail {
        fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
            std::result::Result::Ok(0)
        }
    }

    #[test]
    fn jpeg_propagates_underlying_flush_errors() {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 2, Rgb([1, 2, 3])));
        let error = write_image(&mut FlushFail, &image, ImageFormat::Jpeg).unwrap_err();
        assert!(format!("{error:#}").contains("synthetic underlying flush failure"));
    }

    #[test]
    fn png_and_webp_output_remain_byte_identical() {
        let image = DynamicImage::ImageRgb8(RgbImage::from_fn(31, 29, |x, y| {
            Rgb([x as u8, y as u8, (x ^ y) as u8])
        }));
        for format in [ImageFormat::Png, ImageFormat::WebP] {
            let mut expected = Cursor::new(Vec::new());
            image.write_to(&mut expected, format).unwrap();
            let mut actual = Cursor::new(Vec::new());
            write_image(&mut actual, &image, format).unwrap();
            assert_eq!(actual.into_inner(), expected.into_inner(), "{format:?}");
        }
    }
}

#[cfg(test)]
#[path = "raw_benchmark.rs"]
mod benchmark;
