#![allow(clippy::cognitive_complexity)]

use std::time::Duration;

use thirtyfour::prelude::*;
use tokio::time::Instant;

use price_hunter::browser;
use price_hunter::detect::{self, Product};

const COMPREAHORA_URL: &str = "https://www.compreahora.com.ar/categoria/perfumeria";

#[test]
#[ignore = "requires network and a logged-in browser session"]
fn finds_names_and_prices_on_live_compreahora() {
    let rt = tokio::runtime::Runtime::new().expect("failed to build runtime");
    let _ = rt.block_on(run());
}

async fn run() -> WebDriverResult<()> {
    let driver = browser::launch().await?;
    let result = run_with(&driver).await;
    let _ = driver.quit().await;
    result
}

async fn run_with(driver: &WebDriver) -> WebDriverResult<()> {
    driver
        .goto(COMPREAHORA_URL)
        .await
        .map_err(|e| {
            eprintln!("failed to navigate to {COMPREAHORA_URL}: {e}");
            e
        })?;

    let products = wait_for_products(driver, Duration::from_secs(30)).await?;
    dump_html_if_requested(driver).await?;

    assert!(
        products.len() >= 10,
        "expected at least 10 products, got {}",
        products.len()
    );
    for product in &products {
        assert!(!product.name.is_empty(), "product name empty: {product:?}");
        assert!(product.price > 0.0, "non-positive price: {product:?}");
    }
    let dove = products
        .iter()
        .find(|p| p.name.to_lowercase().contains("dove"));
    assert!(
        dove.is_some(),
        "expected a Dove product among the detected ones: {products:?}"
    );
    Ok(())
}

async fn wait_for_products(driver: &WebDriver, timeout: Duration) -> WebDriverResult<Vec<Product>> {
    let deadline = Instant::now() + timeout;
    loop {
        let source = driver.source().await?;
        if let Some(detection) = detect::detect_grid(&source) {
            println!(
                "detected {} products, container classes: {:?}",
                detection.products.len(),
                detection.container.classes
            );
            return Ok(detection.products);
        }
        if Instant::now() >= deadline {
            panic!("no product grid detected within {timeout:?}");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn dump_html_if_requested(driver: &WebDriver) -> WebDriverResult<()> {
    if std::env::var("PRICE_HUNTER_DUMP_HTML").is_ok_and(|v| v == "1") {
        let source = driver.source().await?;
        let path = "captures/diagnostic/compreahora-live.html";
        if let Some(parent) = std::path::Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, &source);
        println!("dumped live HTML to {path}");
    }
    Ok(())
}
