use std::env;
use std::time::Duration;

use thirtyfour::prelude::*;

#[tokio::main]
async fn main() -> WebDriverResult<()> {
    let driver = WebDriver::managed(DesiredCapabilities::chrome()).await?;

    if let Some(url) = env::args().nth(1) {
        match driver.goto(&url).await {
            Ok(_) => println!("Opened {url}."),
            Err(e) => eprintln!(
                "Could not navigate to {url}: {e}\nThe browser is still open — type the address there."
            ),
        }
    }

    println!("Browser is open and under your control. Close the window (or Ctrl+C) to exit.");

    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if driver.current_url().await.is_err() {
            println!("Browser closed; exiting.");
            break;
        }
    }

    driver.quit().await
}
