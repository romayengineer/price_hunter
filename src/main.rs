use anyhow::Context;
use std::env;
use std::time::Duration;

use thirtyfour::prelude::*;

use price_hunter::browser;
use price_hunter::capture;
use price_hunter::detect;
use price_hunter::detect::{Detection, Product};
use price_hunter::store::Store;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let url = parse_args(&args);
    let store = connect_store()?;
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
    let store = Store::connect().context("cannot connect to PocketBase")?;
    println!("Persisting captures to PocketBase via its API");
    Ok(store)
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
