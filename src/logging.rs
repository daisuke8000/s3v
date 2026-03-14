use std::fs::File;

use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// S3V_LOG 環境変数が設定されている場合にファイルロギングを有効化する。
/// 例: `S3V_LOG=debug cargo run`
pub fn init_logging() -> anyhow::Result<()> {
    if std::env::var("S3V_LOG").is_err() {
        return Ok(());
    }

    let log_dir = std::env::temp_dir();
    let log_path = log_dir.join("s3v.log");
    let log_file = File::create(&log_path)?;

    let file_layer = fmt::layer()
        .with_writer(log_file)
        .with_ansi(false)
        .with_target(true)
        .with_level(true);

    let filter = EnvFilter::try_from_env("S3V_LOG").unwrap_or_else(|_| EnvFilter::new("s3v=debug"));

    tracing_subscriber::registry()
        .with(file_layer)
        .with(filter)
        .init();

    tracing::info!(path = %log_path.display(), "Logging initialized");

    Ok(())
}
