#![allow(clippy::cognitive_complexity)]

use std::time::Duration;

use thirtyfour::prelude::*;

use price_hunter::autoscrape::{self, AutoScrapeOptions};
use price_hunter::browser;

const PARFUMERIE_URL: &str = "https://www.parfumerie.com.ar/fragancias";
const MIN_PRODUCTS: usize = 10;

#[test]
#[ignore = "requires network and a browser session"]
fn auto_scrapes_until_no_growth() {
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
    let options = AutoScrapeOptions {
        url: PARFUMERIE_URL.to_string(),
        ..AutoScrapeOptions::default()
    };
    let mut strategy = autoscrape::strategy_for(PARFUMERIE_URL, &options);
    let detection = autoscrape::scrape_until_no_growth(
        driver,
        strategy.as_mut(),
        Duration::from_millis(500),
        100,
    )
    .await
    .map_err(|e| {
        eprintln!("auto-scrape failed: {e}");
        WebDriverError::FatalError(format!("auto-scrape failed: {e}"))
    })?;

    let products = match detection {
        Some(d) => d.products,
        None => Vec::new(),
    };
    assert!(
        products.len() >= MIN_PRODUCTS,
        "expected at least {MIN_PRODUCTS} products, got {}",
        products.len()
    );
    for product in &products {
        assert!(!product.name.is_empty(), "product name empty: {product:?}");
        assert!(product.price > 0.0, "non-positive price: {product:?}");
        println!("{} - {}", product.name, product.price_text);
    }
    Ok(())
}
