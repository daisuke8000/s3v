use crate::error::{Result, S3vError};

pub fn is_pdf(name: &str) -> bool {
    name.to_lowercase().ends_with(".pdf")
}

/// mutool を使って PDF の指定ページを PNG にレンダリング
pub fn render_page(pdf_bytes: &[u8], page: usize) -> Result<Vec<u8>> {
    let tmp_dir = std::env::temp_dir();
    let pdf_path = tmp_dir.join("s3v_preview.pdf");
    let out_path = tmp_dir.join("s3v_preview_page.png");

    std::fs::write(&pdf_path, pdf_bytes)?;

    let output = std::process::Command::new("mutool")
        .args([
            "draw",
            "-o",
            &out_path.to_string_lossy(),
            "-r",
            "150",
            &pdf_path.to_string_lossy(),
            &format!("{}", page + 1),
        ])
        .output()
        .map_err(|e| {
            S3vError::Terminal(format!(
                "mutool not found: {}. Install with: brew install mupdf-tools",
                e
            ))
        })?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&pdf_path);
        return Err(S3vError::Terminal(format!(
            "mutool failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let png_bytes = std::fs::read(&out_path)?;

    let _ = std::fs::remove_file(&pdf_path);
    let _ = std::fs::remove_file(&out_path);

    Ok(png_bytes)
}

/// PDF の総ページ数を取得
pub fn page_count(pdf_bytes: &[u8]) -> Result<usize> {
    let tmp_dir = std::env::temp_dir();
    let pdf_path = tmp_dir.join("s3v_preview_count.pdf");
    std::fs::write(&pdf_path, pdf_bytes)?;

    let output = std::process::Command::new("mutool")
        .args(["info", &pdf_path.to_string_lossy()])
        .output()
        .map_err(|e| S3vError::Terminal(format!("mutool not found: {}", e)))?;

    let _ = std::fs::remove_file(&pdf_path);

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("Pages:") {
            if let Ok(n) = rest.trim().parse::<usize>() {
                return Ok(n);
            }
        }
    }

    Ok(1)
}
