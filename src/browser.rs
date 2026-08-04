use std::path::PathBuf;

use thirtyfour::common::capabilities::chromium::ChromiumLikeCapabilities;
use thirtyfour::prelude::*;

pub async fn launch() -> WebDriverResult<WebDriver> {
    let profile_dir = profile_dir();
    std::fs::create_dir_all(&profile_dir)?;
    let mut caps = DesiredCapabilities::chrome();
    caps.add_arg(&format!("--user-data-dir={}", profile_dir.display()))?;
    WebDriver::managed(caps).await
}

pub fn profile_dir() -> PathBuf {
    let cwd = std::env::current_dir().expect("could not determine working directory");
    cwd.join("profiles").join("chrome")
}
