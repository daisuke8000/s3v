use crate::error::{Result, S3vError};

pub fn is_pdf(name: &str) -> bool {
    super::has_extension(name, &["pdf"])
}

pub(crate) fn render_page_from_doc_at_dpi(
    doc: &mupdf::Document,
    page: usize,
    dpi: f32,
) -> Result<image::DynamicImage> {
    let pdf_page = doc
        .load_page(page as i32)
        .map_err(|e| S3vError::Pdf(format!("PDF page load error: {e}")))?;

    let scale = dpi / 72.0;
    let matrix = mupdf::Matrix::new_scale(scale, scale);
    let pixmap = pdf_page
        .to_pixmap(&matrix, &mupdf::Colorspace::device_rgb(), false, true)
        .map_err(|e| S3vError::Pdf(format!("PDF render error: {e}")))?;

    let width = pixmap.width();
    let height = pixmap.height();
    let samples = pixmap.samples().to_vec();
    let n = pixmap.n() as u32;

    if n == 4 {
        image::RgbaImage::from_raw(width, height, samples)
            .map(image::DynamicImage::ImageRgba8)
            .ok_or_else(|| S3vError::Pdf("Failed to create image from PDF page".to_string()))
    } else {
        image::RgbImage::from_raw(width, height, samples)
            .map(image::DynamicImage::ImageRgb8)
            .ok_or_else(|| S3vError::Pdf("Failed to create image from PDF page".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_pdf() {
        assert!(is_pdf("document.pdf"));
        assert!(is_pdf("REPORT.PDF"));
        assert!(!is_pdf("image.png"));
    }
}
