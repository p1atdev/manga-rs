use std::{
    io::{BufReader, Cursor, Read},
    path::Path,
};

use anyhow::Result;
use flate2::{bufread::ZlibEncoder, Compression};
use image::{GenericImageView, ImageFormat, ImageReader};
use indicatif::{ParallelProgressIterator, ProgressIterator};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tokio::{fs::File, io::AsyncWriteExt};

use crate::{
    progress::ProgressConfig,
    utils::{self, Bytes},
};

use super::EpisodeWriter;

/// Save as a zip file.
#[derive(Debug, Clone)]
pub struct PdfWriter {
    // num_threads: usize,
    progress: ProgressConfig,
    image_format: image::ImageFormat,
}

impl PdfWriter {
    pub fn new(progress: ProgressConfig, image_format: ImageFormat) -> Self {
        PdfWriter {
            progress,
            image_format,
        }
    }

    pub fn default() -> Self {
        PdfWriter {
            progress: ProgressConfig::default(),
            image_format: image::ImageFormat::Jpeg,
        }
    }
}

impl PdfWriter {
    /// Create a new PDF instance.
    pub fn new_pdf() -> (Pdf, Ref, Ref) {
        let mut pdf = Pdf::new();
        let mut ref_id = Ref::new(1);
        let catalog_id = ref_id.bump().clone();
        let page_tree_id = ref_id.bump().clone();

        // required
        pdf.catalog(catalog_id).pages(page_tree_id);

        (pdf, ref_id, page_tree_id)
    }

    /// Get the image decoder based on the image format.
    fn get_image_decoder(&self) -> pdf_writer::Filter {
        match self.image_format {
            ImageFormat::Jpeg => pdf_writer::Filter::DctDecode,
            _ => pdf_writer::Filter::FlateDecode,
        }
    }

    fn compress_image_bytes_if_needed<B: AsRef<[u8]>>(&self, bytes: B) -> Result<Bytes> {
        match self.image_format {
            ImageFormat::Jpeg => Ok(bytes.as_ref().into()),
            _ => {
                let mut compressed = Vec::new();
                let reader = BufReader::new(bytes.as_ref());
                let mut encoder = ZlibEncoder::new(reader, Compression::default());
                encoder.read_to_end(&mut compressed)?;
                Ok(compressed)
            }
        }
    }

    fn add_image_page(
        &self,
        image_bytes: Bytes,
        image_width: u32,
        image_height: u32,
        pdf: &mut Pdf,
        ref_id: &mut Ref,
        page_tree_id: &Ref,
    ) -> Ref {
        let width = image_width as f32;
        let height = image_height as f32;

        let image_id = ref_id.bump().clone();
        {
            let width = image_width as i32;
            let height = image_height as i32;

            let mut image = pdf.image_xobject(image_id, &image_bytes);
            image.filter(self.get_image_decoder());
            image.width(width);
            image.height(height);
            image.color_space().device_rgb();
            image.bits_per_component(8);
            image.finish();
        }

        // create blank page
        let page_id = ref_id.bump().clone();
        let content_id = ref_id.bump().clone();
        let image_name = format!("Image{}", image_id.get());
        let image_name = Name(image_name.as_bytes());
        {
            let mut page = pdf.page(page_id);
            let area = Rect::new(0.0, 0.0, width, height);
            // let area = Rect::new(0.0, 0.0, 2400., 2400.);
            page.media_box(area);
            page.parent(page_tree_id.clone());
            page.contents(content_id);
            page.resources().x_objects().pair(image_name, image_id);
            page.finish();
        }

        // content
        {
            let mut content = Content::new();
            content.save_state();
            content.transform([width, 0.0, 0.0, height, 0.0, 0.0]);
            content.x_object(image_name);
            pdf.stream(content_id, &content.finish());
        }

        page_id.clone()
    }
}

impl EpisodeWriter for PdfWriter {
    async fn write<P: AsRef<Path>, B: AsRef<[u8]>>(&self, images: Vec<B>, path: P) -> Result<()> {
        let (mut pdf, mut ref_id, page_tree_id) = Self::new_pdf();

        let images: Vec<Bytes> = images
            .into_iter()
            .map(|bytes| bytes.as_ref().into())
            .collect();
        let images_len = images.len();
        let encoded = images
            .into_par_iter()
            .progress_with(
                self.progress
                    .build_with_message(images_len, "Encoding images...")?,
            )
            .map(|image| {
                // get width and height without full decode
                let reader = ImageReader::new(Cursor::new(image.clone())).with_guessed_format()?;
                let (width, height) = reader.into_dimensions()?;
                let image_bytes = self.compress_image_bytes_if_needed(image)?;
                Result::<_>::Ok((image_bytes, width, height))
            })
            .map(|pair| pair.unwrap())
            .collect::<Vec<_>>();

        let page_ids = encoded
            .into_iter()
            .progress_with(
                self.progress
                    .build_with_message(images_len, "Building a PDF...")?,
            )
            .map(|(bytes, width, height)| {
                self.add_image_page(bytes, width, height, &mut pdf, &mut ref_id, &page_tree_id)
            })
            .collect::<Vec<_>>();

        pdf.pages(page_tree_id)
            .count(page_ids.len() as i32)
            .kids(page_ids);

        // save
        let mut file = File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .await?;
        file.write_all(pdf.finish().as_ref()).await?;

        Ok(())
    }

    async fn write_images<P: AsRef<Path>>(
        &self,
        images: Vec<image::DynamicImage>,
        path: P,
    ) -> Result<()> {
        let (mut pdf, mut ref_id, page_tree_id) = Self::new_pdf();

        let image_format = self.image_format;

        let images_len = images.len();
        let encoded = images
            .into_par_iter()
            .progress_with(
                self.progress
                    .build_with_message(images_len, "Encoding images...")?,
            )
            .map(|image| {
                let (width, height) = image.dimensions();
                let bytes = utils::encode_image(&image, image_format)?;
                Result::<_>::Ok((bytes, width, height))
            })
            .map(|pair| pair.unwrap())
            .collect::<Vec<_>>();

        let page_ids = encoded
            .into_iter()
            .progress_with(
                self.progress
                    .build_with_message(images_len, "Building a PDF...")?,
            )
            .map(|(bytes, width, height)| {
                self.add_image_page(bytes, width, height, &mut pdf, &mut ref_id, &page_tree_id)
            })
            .collect::<Vec<_>>();

        pdf.pages(page_tree_id)
            .count(page_ids.len() as i32)
            .kids(page_ids);

        // save
        let mut file = File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .await?;
        file.write_all(pdf.finish().as_ref()).await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use image::GenericImageView;
    use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};

    use super::*;

    #[tokio::test]
    async fn test_pdf_blank_5_pages() -> Result<()> {
        let mut pdf = Pdf::new();
        let mut ref_id = Ref::new(1);
        let catalog_id = ref_id.bump().clone();
        let page_tree_id = ref_id.bump().clone();
        assert_ne!(catalog_id, page_tree_id);

        // catalog
        pdf.catalog(catalog_id).pages(page_tree_id);

        let mut page_ids = vec![];

        // create new page
        for _ in 0..5 {
            let page_id = ref_id.bump().clone();
            let content_id = ref_id.bump().clone();
            let mut page = pdf.page(page_id);

            // create blank page
            let a4 = Rect::new(0.0, 0.0, 595.0, 842.0);
            page.media_box(a4);
            page.parent(page_tree_id);
            page.contents(content_id);
            page.finish();

            // content
            let mut content = Content::new();
            content.begin_text();
            content.save_state();
            content.end_text();
            pdf.stream(content_id, &content.finish());

            page_ids.push(page_id);
        }

        // set page
        pdf.pages(page_tree_id).count(page_ids.len() as i32);
        // .kids(page_ids);

        // save
        tokio::fs::write("playground/output/blank.pdf", pdf.finish()).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_pdf_images() -> Result<()> {
        let mut pdf = Pdf::new();

        let mut ref_id = Ref::new(1);
        let catalog_id = ref_id.bump().clone();
        let page_tree_id = ref_id.bump().clone();
        assert_ne!(catalog_id, page_tree_id);

        // catalog
        pdf.catalog(catalog_id).pages(page_tree_id);

        let mut page_ids = vec![];

        // load the image
        let image_id = ref_id.bump().clone();
        let data = std::fs::read("playground/assets/giga-original.jpg")?;
        let dynamic = image::load_from_memory(&data)?;

        // Write the stream for the image we want to embed.
        let image_name = Name(b"Im0");
        let mut image = pdf.image_xobject(image_id, &data);
        image.filter(Filter::DctDecode);
        image.width(dynamic.width() as i32);
        image.height(dynamic.height() as i32);
        image.color_space().device_rgb();
        image.bits_per_component(8);
        image.finish();

        // create new page
        let page_id = ref_id.bump().clone();
        let content_id = ref_id.bump().clone();
        let mut page = pdf.page(page_id);
        let (width, height) = dynamic.dimensions();
        let area = Rect::new(0.0, 0.0, width as f32, height as f32);
        page.media_box(area);
        page.parent(page_tree_id);
        page.contents(content_id);
        page.resources().x_objects().pair(image_name, image_id);
        page.finish();

        // content
        let mut content = Content::new();
        content.save_state();
        content.transform([width as f32, 0.0, 0.0, height as f32, 0.0, 0.0]);
        content.x_object(image_name);
        pdf.stream(content_id, &content.finish());

        page_ids.push(page_id);

        // set page
        pdf.pages(page_tree_id)
            .count(page_ids.len() as i32)
            .kids(page_ids);

        // save
        tokio::fs::write("playground/output/image.pdf", pdf.finish()).await?;

        Ok(())
    }
}
