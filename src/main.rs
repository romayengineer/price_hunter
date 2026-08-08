use std::env;
use std::time::Duration;

use thirtyfour::prelude::*;

use price_hunter::browser;
use price_hunter::capture;
use price_hunter::detect;
use price_hunter::detect::{Detection, Product};
use price_hunter::store::{self, Store};

#[tokio::main]
async fn main() -> WebDriverResult<()> {
    let args: Vec<String> = env::args().collect();
    let url = parse_args(&args);
    let driver = browser::launch().await?;
    navigate_to_arg(&driver, url).await;

    println!("Browser is open and under your control. Close the window (or Ctrl+C) to exit.");

    let store = connect_store();
    let mut state = LoopState {
        last_source: None,
        detection: None,
        last_capture_products: None,
        store,
    };
    while !poll_closed(&driver).await {
        refresh(&driver, &mut state).await;
    }

    driver.quit().await
}

fn connect_store() -> Option<Store> {
    let default = "traildepot/data/main.db";
    let path = env::var("PRICE_HUNTER_DB").unwrap_or_else(|_| default.to_string());
    match store::Store::open(&path) {
        Ok(store) => {
            println!("Persisting captures to {path} (TrailBase serves this DB)");
            Some(store)
        }
        Err(e) => {
            eprintln!(
                "Could not open {path}: {e}\n\
                 Start TrailBase first (see trailbase/scripts/setup_trailbase.sh) or set \
                 PRICE_HUNTER_DB. Captures will be written to JSON only."
            );
            None
        }
    }
}

fn parse_args(args: &[String]) -> Option<String> {
    args.iter().skip(1).find(|a| !a.starts_with('-')).cloned()
}

struct LoopState {
    last_source: Option<String>,
    detection: Option<Detection>,
    last_capture_products: Option<Vec<Product>>,
    store: Option<Store>,
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
    persist_to_store(&mut state.store, &url, detection);
    state.last_capture_products = Some(detection.products.clone());
}

fn persist_to_store(store: &mut Option<Store>, url: &str, detection: &Detection) {
    let Some(store) = store else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    match store.save(url, now, detection) {
        Ok(()) => println!("Persisted capture to the store"),
        Err(e) => eprintln!("Could not persist capture to the store: {e}"),
    }
}
