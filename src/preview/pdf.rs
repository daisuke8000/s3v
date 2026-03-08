use crate::error::{Result, S3vError};

pub fn is_pdf(name: &str) -> bool {
    name.to_lowercase().ends_with(".pdf")
}

/// PDF のページ数を取得
pub fn page_count(pdf_bytes: &[u8]) -> Result<usize> {
    let tmp_path = write_temp_pdf(pdf_bytes)?;
    let doc = open_document(&tmp_path)?;
    let count = doc
        .page_count()
        .map_err(|e| S3vError::Terminal(format!("PDF page count error: {}", e)))?;
    let _ = std::fs::remove_file(&tmp_path);
    Ok(count as usize)
}

/// PDF の指定ページを DynamicImage にレンダリング
pub fn render_page_to_image(pdf_bytes: &[u8], page: usize) -> Result<image::DynamicImage> {
    let tmp_path = write_temp_pdf(pdf_bytes)?;
    let doc = open_document(&tmp_path)?;
    let _ = std::fs::remove_file(&tmp_path);

    let pdf_page = doc
        .load_page(page as i32)
        .map_err(|e| S3vError::Terminal(format!("PDF page load error: {}", e)))?;

    // 150 DPI (72pt base * 2.08)
    let scale = 150.0 / 72.0;
    let matrix = mupdf::Matrix::new_scale(scale, scale);
    let pixmap = pdf_page
        .to_pixmap(&matrix, &mupdf::Colorspace::device_rgb(), false, true)
        .map_err(|e| S3vError::Terminal(format!("PDF render error: {}", e)))?;

    let width = pixmap.width();
    let height = pixmap.height();
    let samples = pixmap.samples().to_vec();
    let n = pixmap.n() as u32;

    if n == 4 {
        image::RgbaImage::from_raw(width, height, samples)
            .map(image::DynamicImage::ImageRgba8)
            .ok_or_else(|| S3vError::Terminal("Failed to create image from PDF page".to_string()))
    } else {
        image::RgbImage::from_raw(width, height, samples)
            .map(image::DynamicImage::ImageRgb8)
            .ok_or_else(|| S3vError::Terminal("Failed to create image from PDF page".to_string()))
    }
}

fn write_temp_pdf(pdf_bytes: &[u8]) -> Result<std::path::PathBuf> {
    let tmp_path = std::env::temp_dir().join("s3v_preview.pdf");
    std::fs::write(&tmp_path, pdf_bytes)?;
    Ok(tmp_path)
}

fn open_document(path: &std::path::Path) -> Result<mupdf::Document> {
    mupdf::Document::open(path).map_err(|e| S3vError::Terminal(format!("PDF open error: {}", e)))
}
