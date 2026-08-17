#![allow(clippy::cognitive_complexity)]

use std::time::Duration;

use thirtyfour::prelude::*;
use tokio::time::Instant;

use price_hunter::browser;
use price_hunter::detect::{self, Product};

const PARFUMERIE_URL: &str = "https://www.parfumerie.com.ar/fragancias";
const MIN_PRODUCTS: usize = 10;

#[test]
#[ignore = "requires network and a browser session"]
fn finds_names_and_prices_on_live_parfumerie() {
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
    driver.goto(PARFUMERIE_URL).await.map_err(|e| {
        eprintln!("failed to navigate to {PARFUMERIE_URL}: {e}");
        e
    })?;

    let products = wait_for_products(driver, Duration::from_secs(60)).await?;
    dump_html_if_requested(driver).await?;

    assert!(
        products.len() >= MIN_PRODUCTS,
        "expected at least {MIN_PRODUCTS} products, got {}",
        products.len()
    );
    for product in &products {
        assert!(!product.name.is_empty(), "product name empty: {product:?}");
        assert!(product.price > 0.0, "non-positive price: {product:?}");
    }
    for product in &products {
        println!("{} - {}", product.name, product.price_text);
    }
    Ok(())
}

async fn wait_for_products(driver: &WebDriver, timeout: Duration) -> WebDriverResult<Vec<Product>> {
    let deadline = Instant::now() + timeout;
    let mut best: Vec<Product> = Vec::new();
    loop {
        scroll_to_bottom(driver).await?;
        let source = driver.source().await?;
        if let Some(detection) = detect::detect_grid(&source) {
            println!(
                "detected {} products, container classes: {:?}",
                detection.products.len(),
                detection.container.classes
            );
            if detection.products.len() > best.len() {
                best = detection.products;
            }
            if best.len() >= MIN_PRODUCTS {
                return Ok(best);
            }
        }
        if Instant::now() >= deadline {
            panic!(
                "fewer than {MIN_PRODUCTS} products detected within {timeout:?}, got {}",
                best.len()
            );
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn scroll_to_bottom(driver: &WebDriver) -> WebDriverResult<()> {
    let _ = driver
        .execute(
            "window.scrollTo(0, document.body.scrollHeight)",
            Vec::<serde_json::Value>::new(),
        )
        .await;
    Ok(())
}

async fn dump_html_if_requested(driver: &WebDriver) -> WebDriverResult<()> {
    if std::env::var("PRICE_HUNTER_DUMP_HTML").is_ok_and(|v| v == "1") {
        let source = driver.source().await?;
        let path = "captures/diagnostic/parfumerie-live.html";
        if let Some(parent) = std::path::Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, &source);
        println!("dumped live HTML to {path}");
    }
    Ok(())
}
