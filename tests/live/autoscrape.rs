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

/// Extracts the page number from a `memory optimization: reloading ...?page=N ...`
/// log line, if present.
fn parse_page_from_reload(line: &str) -> Option<u32> {
    let marker = "memory optimization: reloading";
    let idx = line.find(marker)?;
    let rest = &line[idx..];
    // Match both "?page=" (first param) and "&page=" (subsequent param)
    let page_idx = rest.find("?page=").or_else(|| rest.find("&page="))?;
    let after = &rest[page_idx + 6..]; // len("?page=") == len("&page/") == 6
    let number: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    number.parse::<u32>().ok()
}

/// Reads the spawned process's stderr until it exits or `timeout` elapses,
/// returning `(max_product_count, max_page_reached, window_reload_fired)`.
///
/// - `max_product_count` is the largest `best = N` count seen.
/// - `max_page_reached` is the highest page number from reload messages.
/// - `window_reload_fired` is true when at least one reload message appeared.
fn collect_window_stats(child: &mut Child, timeout: Duration) -> (usize, u32, bool) {
    let stderr = child.stderr.take().expect("stderr not captured");
    let deadline = std::time::Instant::now() + timeout;
    let mut max_count = 0usize;
    let mut max_page = 0u32;
    let mut has_reload = false;

    let mut reader = BufReader::new(stderr);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buffer);
                if let Some(count) = parse_best_count(&line) {
                    max_count = max_count.max(count);
                }
                if let Some(page) = parse_page_from_reload(&line) {
                    has_reload = true;
                    max_page = max_page.max(page);
                }
            }
            Err(_) => break,
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
    }
    (max_count, max_page, has_reload)
}

/// Runs the `pricehunter` binary against perfumeriasrouge with `-strategy page`
/// and `-window-threshold 100` to verify the memory-optimization windowing
/// feature. Asserts that:
///
/// 1. Page 50 is reached (the site has 50 pages).
/// 2. At least one window reload fires (the feature activates).
#[test]
#[ignore = "requires network and a browser session"]
fn windowed_auto_scrape_reaches_page_50() {
    let bin = env!("CARGO_BIN_EXE_pricehunter");
    let mut child = Command::new(bin)
        .args([
            "-auto-scrape",
            PERFUMERIASROUGE_URL,
            "-strategy",
            "page",
            "-page-param",
            "page",
            "-window-threshold",
            "100",
            "-headless",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn pricehunter");

    let (max_count, max_page, has_reload) =
        collect_window_stats(&mut child, Duration::from_secs(600));

    let _ = child.kill();
    let _ = child.wait();

    assert!(
        max_page >= 50,
        "expected to reach page 50, got page {max_page} (max products: {max_count}, \
         window reload fired: {has_reload})"
    );
    assert!(
        has_reload,
        "window reload never fired — the memory-optimization feature did not activate \
         (max page: {max_page}, max products: {max_count})"
    );
    println!(
        "windowed auto-scrape passed: reached page {max_page} with {max_count} products, \
         window reload fired"
    );
}

#[cfg(test)]
mod tests {
    use super::{parse_best_count, parse_page_from_reload};

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

    #[test]
    fn parses_page_from_reload_log_line() {
        assert_eq!(
            parse_page_from_reload(
                "memory optimization: reloading https://www.perfumeriasrouge.com/perfumes-y-fragancias?page=50 (window threshold 100 reached with 120 products)"
            ),
            Some(50)
        );
        assert_eq!(
            parse_page_from_reload(
                "memory optimization: reloading https://example.com/list?sort=price&page=12 (window threshold 100 reached with 105 products)"
            ),
            Some(12)
        );
    }

    #[test]
    fn ignores_unrelated_reload_lines() {
        assert_eq!(
            parse_page_from_reload("auto-scrape step 3: best = 60 products"),
            None
        );
        assert_eq!(
            parse_page_from_reload("Navigating to https://example.com"),
            None
        );
        assert_eq!(parse_page_from_reload(""), None);
    }
}
