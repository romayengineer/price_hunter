mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init()
        .ok();
    let args: Vec<String> = std::env::args().collect();
    cli::run(cli::parse(&args)).await
}
