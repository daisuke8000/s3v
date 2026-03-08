use crate::preview::page_cache::CACHE_WINDOW_RADIUS;
use crate::preview::pdf;

pub const PDF_RENDER_DPI: f32 = 96.0;

pub enum WorkerRequest {
    RenderPage(usize),
    RenderBatch(Vec<usize>),
    Shutdown,
}

pub enum WorkerResponse {
    PageRendered {
        page: usize,
        image: image::DynamicImage,
    },
    InitComplete {
        total_pages: usize,
        first_page: image::DynamicImage,
    },
    Error {
        page: usize,
        error: String,
    },
}

pub struct PdfWorker {
    request_tx: std::sync::mpsc::Sender<WorkerRequest>,
    pub response_rx: tokio::sync::mpsc::UnboundedReceiver<WorkerResponse>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl PdfWorker {
    pub fn spawn(pdf_bytes: Vec<u8>) -> Self {
        let (req_tx, req_rx) = std::sync::mpsc::channel::<WorkerRequest>();
        let (resp_tx, resp_rx) = tokio::sync::mpsc::unbounded_channel::<WorkerResponse>();

        let handle = std::thread::spawn(move || {
            worker_thread(pdf_bytes, req_rx, resp_tx);
        });

        Self {
            request_tx: req_tx,
            response_rx: resp_rx,
            handle: Some(handle),
        }
    }

    pub fn request_page(&self, page: usize) {
        let _ = self.request_tx.send(WorkerRequest::RenderPage(page));
    }

    pub fn request_batch(&self, pages: Vec<usize>) {
        if !pages.is_empty() {
            let _ = self.request_tx.send(WorkerRequest::RenderBatch(pages));
        }
    }

    pub fn try_recv(&mut self) -> Option<WorkerResponse> {
        self.response_rx.try_recv().ok()
    }
}

impl Drop for PdfWorker {
    fn drop(&mut self) {
        let _ = self.request_tx.send(WorkerRequest::Shutdown);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn worker_thread(
    pdf_bytes: Vec<u8>,
    req_rx: std::sync::mpsc::Receiver<WorkerRequest>,
    resp_tx: tokio::sync::mpsc::UnboundedSender<WorkerResponse>,
) {
    let doc = match mupdf::Document::from_bytes(&pdf_bytes, "application/pdf") {
        Ok(d) => d,
        Err(e) => {
            let _ = resp_tx.send(WorkerResponse::Error {
                page: 0,
                error: format!("PDF open error: {e}"),
            });
            return;
        }
    };

    let total_pages = match doc.page_count() {
        Ok(n) => n as usize,
        Err(e) => {
            let _ = resp_tx.send(WorkerResponse::Error {
                page: 0,
                error: format!("PDF page count error: {e}"),
            });
            return;
        }
    };

    // page 0 レンダリング → InitComplete 送信
    match pdf::render_page_from_doc_at_dpi(&doc, 0, PDF_RENDER_DPI) {
        Ok(img) => {
            let _ = resp_tx.send(WorkerResponse::InitComplete {
                total_pages,
                first_page: img,
            });
        }
        Err(e) => {
            let _ = resp_tx.send(WorkerResponse::Error {
                page: 0,
                error: e.to_string(),
            });
            return;
        }
    }

    // pages 1..=CACHE_WINDOW_RADIUS 先読みレンダリング
    let prefetch_end = CACHE_WINDOW_RADIUS.min(total_pages.saturating_sub(1));
    for p in 1..=prefetch_end {
        // 優先リクエストを先に処理
        if let Ok(req) = req_rx.try_recv()
            && handle_request(&doc, req, &resp_tx, &req_rx)
        {
            return;
        }
        render_and_send(&doc, p, &resp_tx);
    }

    // メインループ: リクエスト待機
    while let Ok(req) = req_rx.recv() {
        if handle_request(&doc, req, &resp_tx, &req_rx) {
            return;
        }
    }
}

/// リクエストを処理。Shutdown なら true を返す
fn handle_request(
    doc: &mupdf::Document,
    req: WorkerRequest,
    resp_tx: &tokio::sync::mpsc::UnboundedSender<WorkerResponse>,
    req_rx: &std::sync::mpsc::Receiver<WorkerRequest>,
) -> bool {
    match req {
        WorkerRequest::Shutdown => true,
        WorkerRequest::RenderPage(page) => {
            render_and_send(doc, page, resp_tx);
            false
        }
        WorkerRequest::RenderBatch(pages) => {
            for p in pages {
                // バッチ中も優先リクエスト確認
                if let Ok(priority_req) = req_rx.try_recv() {
                    match priority_req {
                        WorkerRequest::Shutdown => return true,
                        WorkerRequest::RenderPage(urgent) => {
                            render_and_send(doc, urgent, resp_tx);
                        }
                        WorkerRequest::RenderBatch(more) => {
                            for mp in more {
                                render_and_send(doc, mp, resp_tx);
                            }
                        }
                    }
                }
                render_and_send(doc, p, resp_tx);
            }
            false
        }
    }
}

fn render_and_send(
    doc: &mupdf::Document,
    page: usize,
    resp_tx: &tokio::sync::mpsc::UnboundedSender<WorkerResponse>,
) {
    match pdf::render_page_from_doc_at_dpi(doc, page, PDF_RENDER_DPI) {
        Ok(img) => {
            let _ = resp_tx.send(WorkerResponse::PageRendered { page, image: img });
        }
        Err(e) => {
            let _ = resp_tx.send(WorkerResponse::Error {
                page,
                error: e.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用の最小 PDF を生成
    fn create_test_pdf(num_pages: usize) -> Vec<u8> {
        let mut pdf_data = Vec::new();
        pdf_data.extend_from_slice(b"%PDF-1.4\n");

        let mut offsets = Vec::new();

        // Object 1: Catalog
        offsets.push(pdf_data.len());
        pdf_data.extend_from_slice(b"1 0 obj\n<</Type /Catalog /Pages 2 0 R>>\nendobj\n");

        // Object 2: Pages
        offsets.push(pdf_data.len());
        let kids: Vec<String> = (0..num_pages).map(|i| format!("{} 0 R", i + 3)).collect();
        let pages_obj = format!(
            "2 0 obj\n<</Type /Pages /Kids [{}] /Count {}>>\nendobj\n",
            kids.join(" "),
            num_pages
        );
        pdf_data.extend_from_slice(pages_obj.as_bytes());

        // Objects 3..N: individual pages
        for i in 0..num_pages {
            offsets.push(pdf_data.len());
            let page_obj = format!(
                "{} 0 obj\n<</Type /Page /Parent 2 0 R /MediaBox [0 0 72 72]>>\nendobj\n",
                i + 3
            );
            pdf_data.extend_from_slice(page_obj.as_bytes());
        }

        // Cross-reference table
        let xref_offset = pdf_data.len();
        pdf_data.extend_from_slice(b"xref\n");
        let total = offsets.len() + 1;
        pdf_data.extend_from_slice(format!("0 {total}\n").as_bytes());
        pdf_data.extend_from_slice(b"0000000000 65535 f \n");
        for offset in &offsets {
            pdf_data.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }

        // Trailer
        pdf_data.extend_from_slice(b"trailer\n");
        pdf_data.extend_from_slice(format!("<</Size {total} /Root 1 0 R>>\n").as_bytes());
        pdf_data.extend_from_slice(b"startxref\n");
        pdf_data.extend_from_slice(format!("{xref_offset}\n").as_bytes());
        pdf_data.extend_from_slice(b"%%EOF\n");

        pdf_data
    }

    #[test]
    fn test_worker_init_and_shutdown() {
        let pdf_bytes = create_test_pdf(3);
        let mut worker = PdfWorker::spawn(pdf_bytes);

        // InitComplete を受信
        let resp = loop {
            if let Some(r) = worker.try_recv() {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };

        match resp {
            WorkerResponse::InitComplete {
                total_pages,
                first_page: _,
            } => {
                assert_eq!(total_pages, 3);
            }
            WorkerResponse::Error { error, .. } => {
                panic!("Worker returned error: {error}");
            }
            _ => panic!("Expected InitComplete"),
        }

        // Drop で Shutdown される
        drop(worker);
    }

    #[test]
    fn test_worker_render_page() {
        let pdf_bytes = create_test_pdf(5);
        let mut worker = PdfWorker::spawn(pdf_bytes);

        // InitComplete を待つ
        loop {
            if let Some(WorkerResponse::InitComplete { .. }) = worker.try_recv() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // 先読みレスポンスを消費
        std::thread::sleep(std::time::Duration::from_millis(100));
        while worker.try_recv().is_some() {}

        // ページ 4 をリクエスト
        worker.request_page(4);

        let resp = loop {
            if let Some(r) = worker.try_recv() {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };

        match resp {
            WorkerResponse::PageRendered { page, .. } => {
                assert_eq!(page, 4);
            }
            WorkerResponse::Error { error, .. } => {
                panic!("Render error: {error}");
            }
            _ => panic!("Expected PageRendered"),
        }
    }

    #[test]
    fn test_worker_batch_render() {
        let pdf_bytes = create_test_pdf(5);
        let mut worker = PdfWorker::spawn(pdf_bytes);

        // InitComplete を待つ
        loop {
            if let Some(WorkerResponse::InitComplete { .. }) = worker.try_recv() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // 先読みレスポンスを消費
        std::thread::sleep(std::time::Duration::from_millis(100));
        while worker.try_recv().is_some() {}

        // バッチリクエスト
        worker.request_batch(vec![3, 4]);

        let mut rendered_pages = Vec::new();
        let start = std::time::Instant::now();
        while rendered_pages.len() < 2 && start.elapsed() < std::time::Duration::from_secs(5) {
            if let Some(resp) = worker.try_recv() {
                if let WorkerResponse::PageRendered { page, .. } = resp {
                    rendered_pages.push(page);
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        rendered_pages.sort_unstable();
        assert_eq!(rendered_pages, vec![3, 4]);
    }

    #[test]
    fn test_worker_invalid_pdf() {
        let mut worker = PdfWorker::spawn(b"not a pdf".to_vec());

        let resp = loop {
            if let Some(r) = worker.try_recv() {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };

        match resp {
            WorkerResponse::Error { error, .. } => {
                assert!(error.contains("PDF"));
            }
            _ => panic!("Expected Error for invalid PDF"),
        }
    }
}
