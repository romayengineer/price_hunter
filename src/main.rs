use std::env;
use std::path::PathBuf;
use std::time::Duration;

use thirtyfour::common::capabilities::chromium::ChromiumLikeCapabilities;
use thirtyfour::prelude::*;

use price_hunter::capture;
use price_hunter::detect;
use price_hunter::detect::{Detection, Product};

#[tokio::main]
async fn main() -> WebDriverResult<()> {
    let args: Vec<String> = env::args().collect();
    let url = parse_args(&args);
    let driver = launch_driver().await?;
    navigate_to_arg(&driver, url).await;

    println!("Browser is open and under your control. Close the window (or Ctrl+C) to exit.");

    let mut state = LoopState {
        last_source: None,
        detection: None,
        last_capture_products: None,
    };
    while !poll_closed(&driver).await {
        refresh(&driver, &mut state).await;
    }

    driver.quit().await
}

fn parse_args(args: &[String]) -> Option<String> {
    args.iter().skip(1).find(|a| !a.starts_with('-')).cloned()
}

async fn launch_driver() -> WebDriverResult<WebDriver> {
    let profile_dir = profile_dir();
    std::fs::create_dir_all(&profile_dir)?;
    let mut caps = DesiredCapabilities::chrome();
    caps.add_arg(&format!("--user-data-dir={}", profile_dir.display()))?;
    WebDriver::managed(caps).await
}

fn profile_dir() -> PathBuf {
    let cwd = std::env::current_dir().expect("could not determine working directory");
    cwd.join("profiles").join("chrome")
}

struct LoopState {
    last_source: Option<String>,
    detection: Option<Detection>,
    last_capture_products: Option<Vec<Product>>,
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
    state.last_capture_products = Some(detection.products.clone());
}
