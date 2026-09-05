mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Suppress noisy `thirtyfour` webdriver spans (`run_webdriver_cmd` at INFO + request/response at DEBUG).
    // `RUST_LOG` still overrides, e.g. `RUST_LOG=thirtyfour=debug` to re-enable.
    let filter = {
        let base_raw = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
        let base = base_raw.trim();
        let base = if base.is_empty() { "info" } else { base };
        let filter_str = if base.contains("thirtyfour") {
            base.to_string()
        } else {
            format!("{base},thirtyfour=warn")
        };
        tracing_subscriber::EnvFilter::new(filter_str)
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
    // Also init `log` bridge so `log::info!/error!` from the codebase still appears (via tracing).
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();
    let args: Vec<String> = std::env::args().collect();
    let yes = cli::wants_yes(&args);
    cli::run(cli::parse(&args), yes).await
}
