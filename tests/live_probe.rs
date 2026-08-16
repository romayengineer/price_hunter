#![allow(clippy::cognitive_complexity)]

use std::time::Duration;

use thirtyfour::prelude::*;
use tokio::time::Instant;

use price_hunter::browser;
use price_hunter::detect;

#[test]
#[ignore = "diagnostic, requires network and a browser session"]
fn live_probe() {
    let rt = tokio::runtime::Runtime::new().expect("failed to build runtime");
    rt.block_on(run()).expect("live probe failed");
}

async fn run() -> WebDriverResult<()> {
    let url = std::env::var("PRICE_HUNTER_LIVE_URL")
        .unwrap_or_else(|_| "https://www.parfumerie.com.ar/fragancias".to_string());
    let driver = browser::launch().await?;
    let result = run_with(&driver, &url).await;
    let _ = driver.quit().await;
    result
}

async fn run_with(driver: &WebDriver, url: &str) -> WebDriverResult<()> {
    driver.goto(url).await.map_err(|e| {
        eprintln!("failed to navigate to {url}: {e}");
        e
    })?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        let _ = driver
            .execute(
                "window.scrollTo(0, document.body.scrollHeight)",
                Vec::<serde_json::Value>::new(),
            )
            .await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let source = driver.source().await?;
    if std::env::var("PRICE_HUNTER_DUMP_HTML").is_ok_and(|v| v == "1") {
        let path = "captures/diagnostic/live-probe.html";
        if let Some(parent) = std::path::Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, &source);
        println!("dumped live HTML to {path}");
    }
    match detect::detect_grid(&source) {
        Some(detection) => {
            println!("container: {:?}", detection.container.classes);
            for product in &detection.products {
                println!("{} - {}", product.name, product.price_text);
            }
        }
        None => println!("no grid detected"),
    }
    Ok(())
}
