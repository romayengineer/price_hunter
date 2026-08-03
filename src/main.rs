use std::env;
use std::time::Duration;

use thirtyfour::prelude::*;

#[tokio::main]
async fn main() -> WebDriverResult<()> {
    let driver = WebDriver::managed(DesiredCapabilities::chrome()).await?;
    navigate_to_arg(&driver, env::args().nth(1)).await;

    println!("Browser is open and under your control. Close the window (or Ctrl+C) to exit.");

    wait_for_close(&driver).await;
    driver.quit().await
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

async fn wait_for_close(driver: &WebDriver) {
    while !poll_closed(driver).await {}
    println!("Browser closed; exiting.");
}

async fn poll_closed(driver: &WebDriver) -> bool {
    tokio::time::sleep(Duration::from_secs(2)).await;
    driver.current_url().await.is_err()
}
