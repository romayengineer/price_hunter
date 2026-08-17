//! Launches Chrome (via `thirtyfour`'s managed WebDriver) using a persistent
//! profile so logins/cookies survive between runs.

use std::path::PathBuf;

use thirtyfour::common::capabilities::chromium::ChromiumLikeCapabilities;
use thirtyfour::prelude::*;

/// Launches Chrome with the project's persistent profile and returns a
/// `WebDriver` handle to it.
pub async fn launch() -> WebDriverResult<WebDriver> {
    let profile_dir = profile_dir();
    std::fs::create_dir_all(&profile_dir)?;
    let mut caps = DesiredCapabilities::chrome();
    caps.add_arg(&format!("--user-data-dir={}", profile_dir.display()))?;
    WebDriver::managed(caps).await
}

/// The persistent Chrome profile directory (`<cwd>/profiles/chrome`).
pub fn profile_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    cwd.join("profiles").join("chrome")
}
