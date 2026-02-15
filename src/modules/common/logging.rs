use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_logging(verbose: bool) {
    let level = if verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    // 创建日志目录
    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("rust-yu")
        .join("logs");

    let _ = std::fs::create_dir_all(&log_dir);

    // 设置文件输出
    let file_appender = tracing_appender::rolling::daily(&log_dir, "rust-yu.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // 保持 guard 存活
    std::mem::forget(_guard);

    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(format!(
            "rust_yu={},info",
            level
        )))
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr));

    let _ = subscriber.try_init();
}
