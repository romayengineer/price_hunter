#![allow(clippy::cognitive_complexity)]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use thirtyfour::prelude::*;

use price_hunter::autoscrape::{self, AutoScrapeOptions};
use price_hunter::browser;

const PARFUMERIE_URL: &str = "https://www.parfumerie.com.ar/fragancias";
const MIN_PRODUCTS: usize = 10;

const PERFUMERIASROUGE_URL: &str = "https://www.perfumeriasrouge.com/perfumes-y-fragancias";

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
        Duration::from_secs(10),
        100,
        |_| {},
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

/// Runs the real `pricehunter` binary against perfumeriasrouge and asserts that
/// the auto-scrape actually clicks the "Mostrar más" load-more button, proven
/// by the detected product count growing well past the initial page. The
/// process is killed once the count clears the threshold, so the test is
/// bounded even though the site's catalog is large.
#[test]
#[ignore = "requires network and a browser session"]
fn binary_auto_scrape_clicks_load_more_button() {
    let bin = env!("CARGO_BIN_EXE_pricehunter");
    let mut child = Command::new(bin)
        .args(["-auto-scrape", PERFUMERIASROUGE_URL, "-headless"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn pricehunter");

    // The auto-scrape progress lines are `log::info!`, which env_logger writes
    // to stderr.
    let max_count = collect_max_product_count(&mut child, Duration::from_secs(90));

    let _ = child.kill();
    let _ = child.wait();

    // The initial page renders ~6 products; clearing this threshold requires the
    // load-more button to have been clicked and more products to have loaded.
    assert!(
        max_count > 30,
        "expected product count to grow past the initial page (button not clicked?), \
         got {max_count} products"
    );
    println!("load-more button clicked: product count grew to {max_count}");
}

/// Reads the spawned process's stderr until it exits or `timeout` elapses,
/// returning the largest "best = N products" count seen in the auto-scrape log
/// lines.
fn collect_max_product_count(child: &mut Child, timeout: Duration) -> usize {
    let stderr = child.stderr.take().expect("stderr not captured");
    let deadline = std::time::Instant::now() + timeout;
    let mut max_count = 0usize;

    let mut reader = BufReader::new(stderr);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) => break, // EOF: process exited
            Ok(_) => {
                let line = String::from_utf8_lossy(&buffer);
                if let Some(count) = parse_best_count(&line) {
                    max_count = max_count.max(count);
                }
            }
            Err(_) => break,
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
    }
    max_count
}

/// Extracts the product count from an `auto-scrape step N: best = M products`
/// log line, if present.
fn parse_best_count(line: &str) -> Option<usize> {
    let marker = "auto-scrape step";
    let idx = line.find(marker)?;
    let rest = &line[idx..];
    let best_idx = rest.find("best = ")?;
    let after = &rest[best_idx + "best = ".len()..];
    let number: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    number.parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::parse_best_count;

    #[test]
    fn parses_best_count_from_log_line() {
        assert_eq!(
            parse_best_count("2026-08-19T04:00:00Z INFO  price_hunter::infrastructure::autoscrape] auto-scrape step 3: best = 60 products"),
            Some(60)
        );
        assert_eq!(
            parse_best_count("auto-scrape step 0: best = 6 products"),
            Some(6)
        );
    }

    #[test]
    fn ignores_unrelated_lines() {
        assert_eq!(parse_best_count("Navigating to https://example.com"), None);
        assert_eq!(parse_best_count("auto-scrape step 1: no best marker"), None);
        assert_eq!(parse_best_count(""), None);
    }
}
