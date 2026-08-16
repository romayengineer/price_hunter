use anyhow::Context;
use std::env;
use std::time::Duration;

use thirtyfour::prelude::*;

use price_hunter::browser;
use price_hunter::capture;
use price_hunter::config;
use price_hunter::detect;
use price_hunter::detect::{Detection, Product};
use price_hunter::export;
use price_hunter::instance::InstanceGuard;
use price_hunter::services;
use price_hunter::store::Store;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if let Some(path) = import_products_arg(&args) {
        return import_products(&path);
    }
    if let Some(path) = import_brands_arg(&args) {
        return import_brands(&path);
    }
    if let Some(path) = export_matrix_arg(&args) {
        return export_matrix(&path);
    }
    if args.iter().skip(1).any(|a| a == "-match-products") {
        return match_products();
    }
    if args.iter().skip(1).any(|a| a == "-link-matches") {
        return link_matches();
    }
    if args.iter().skip(1).any(|a| a == "-match-brands") {
        return match_brands();
    }
    if args.iter().skip(1).any(|a| a == "-matrix-server") {
        return matrix_server().await;
    }
    let url = parse_args(&args);
    let store = connect_store()?;
    let _instance = InstanceGuard::acquire().context("cannot take single-instance lock")?;
    let driver = browser::launch().await?;
    navigate_to_arg(&driver, url).await;

    println!("Browser is open and under your control. Close the window (or Ctrl+C) to exit.");

    let mut state = LoopState {
        last_source: None,
        detection: None,
        last_capture_products: None,
        store,
    };
    while !poll_closed(&driver).await {
        refresh(&driver, &mut state).await;
    }

    driver.quit().await.map_err(Into::into)
}

fn connect_store() -> anyhow::Result<Store> {
    config::Config::ensure_template();
    let store = Store::connect().context("cannot connect to PocketBase")?;
    println!("Persisting captures to PocketBase via its API");
    Ok(store)
}

/// Returns the CSV path when `-import-products <file>` is present.
fn import_products_arg(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .position(|a| a == "-import-products")
        .and_then(|i| args.get(i + 2).cloned())
}

/// Returns the CSV path when `-import-brands <file>` is present.
fn import_brands_arg(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .position(|a| a == "-import-brands")
        .and_then(|i| args.get(i + 2).cloned())
}

/// Returns the target path when `-export-matrix <file>` is present.
fn export_matrix_arg(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .position(|a| a == "-export-matrix")
        .and_then(|i| args.get(i + 2).cloned())
}

/// Imports `brand,name,size` rows from a CSV into the `products` table and
/// exits without opening a browser.
fn import_products(path: &str) -> anyhow::Result<()> {
    config::Config::ensure_template();
    let store = Store::connect().context("cannot connect to PocketBase")?;
    let created = store.import_products_csv(std::path::Path::new(path))?;
    println!("Done: {created} products imported");
    Ok(())
}

/// Imports the canonical brand list (single CSV column) into the `brand`
/// table and exits without opening a browser.
fn import_brands(path: &str) -> anyhow::Result<()> {
    config::Config::ensure_template();
    let store = Store::connect().context("cannot connect to PocketBase")?;
    let created = store.import_brands_csv(std::path::Path::new(path))?;
    println!("Done: {created} brands imported");
    Ok(())
}

/// Writes the product × provider price matrix (same table the matrix server
/// serves) to a CSV file and exits without opening a browser.
fn export_matrix(path: &str) -> anyhow::Result<()> {
    config::Config::ensure_template();
    let store = Store::connect().context("cannot connect to PocketBase")?;
    let matrix = services::matrix::matrix(&store)?;
    let csv = export::matrix_to_csv(&matrix)?;
    std::fs::write(path, csv).with_context(|| format!("could not write CSV to {path}"))?;
    println!(
        "Exported {} products × {} providers to {path}",
        matrix.rows.len(),
        matrix.providers.len()
    );
    Ok(())
}

/// Runs the fuzzy matcher against the `products` and `provider_products`
/// tables and exits without opening a browser.
fn match_products() -> anyhow::Result<()> {
    config::Config::ensure_template();
    let store = Store::connect().context("cannot connect to PocketBase")?;
    let matched = services::matching::match_products(&store)?;
    println!("Done: {matched} provider products matched");
    Ok(())
}

/// Re-links provider products from already-stored comparisons (no backfill)
/// and exits without opening a browser.
fn link_matches() -> anyhow::Result<()> {
    config::Config::ensure_template();
    let store = Store::connect().context("cannot connect to PocketBase")?;
    let matched = services::matching::link_matches(&store)?;
    println!("Done: {matched} provider products matched");
    Ok(())
}

/// Assigns a brand to every provider product (`provider_products.brand_id`,
/// from the linked product's brand or a fuzzy brand match) and exits without
/// opening a browser.
fn match_brands() -> anyhow::Result<()> {
    config::Config::ensure_template();
    let store = Store::connect().context("cannot connect to PocketBase")?;
    services::brands::match_brands(&store)?;
    println!("Done: brand matching complete");
    Ok(())
}

/// Serves the product × provider price matrix on http://127.0.0.1:8091 and
/// keeps running until interrupted.
async fn matrix_server() -> anyhow::Result<()> {
    config::Config::ensure_template();
    let store = Store::connect().context("cannot connect to PocketBase")?;
    price_hunter::matrix_server::serve(store).await
}

fn parse_args(args: &[String]) -> Option<String> {
    args.iter().skip(1).find(|a| !a.starts_with('-')).cloned()
}

struct LoopState {
    last_source: Option<String>,
    detection: Option<Detection>,
    last_capture_products: Option<Vec<Product>>,
    store: Store,
}

async fn navigate_to_arg(driver: &WebDriver, url: Option<String>) {
    let Some(url) = url else {
        return;
    };
    open(driver, &url).await;
}

async fn open(driver: &WebDriver, url: &str) {
    match driver.goto(url).await {
        Ok(_) => println!("Opened {url}."),
        Err(e) => eprintln!(
            "Could not navigate to {url}: {e}\nThe browser is still open — type the address there."
        ),
    }
}

async fn poll_closed(driver: &WebDriver) -> bool {
    tokio::time::sleep(Duration::from_secs(2)).await;
    driver.current_url().await.is_err()
}

async fn refresh(driver: &WebDriver, state: &mut LoopState) {
    let source = driver.source().await.ok();
    update_state(state, source);
    capture_if_needed(driver, state).await;
}

fn update_state(state: &mut LoopState, source: Option<String>) {
    let Some(source) = source else {
        return;
    };
    if state.last_source.as_deref() == Some(source.as_str()) {
        return;
    }
    state.last_source = Some(source.clone());
    if let Some(detection) = detect::detect_grid(&source) {
        state.detection = Some(detection);
    }
}

async fn capture_if_needed(driver: &WebDriver, state: &mut LoopState) {
    let Some(detection) = &state.detection else {
        return;
    };
    if state.last_capture_products.as_ref() == Some(&detection.products) {
        return;
    }
    let url = driver
        .current_url()
        .await
        .map(|u| u.to_string())
        .unwrap_or_default();
    let path = capture::write_capture("captures", &url, detection);
    println!(
        "Captured {} products to {}",
        detection.products.len(),
        path.display()
    );
    let capture_path = path.display().to_string();
    persist_to_store(&state.store, &url, &capture_path, detection);
    state.last_capture_products = Some(detection.products.clone());
}

fn persist_to_store(store: &Store, url: &str, capture_path: &str, detection: &Detection) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    match store.save(url, now, capture_path, detection) {
        Ok(()) => println!("Persisted capture to the store"),
        Err(e) => eprintln!("Could not persist capture to the store: {e}"),
    }
}
