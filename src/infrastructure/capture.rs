use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::domain::detect::{Container, Detection, Product};

#[derive(Serialize)]
struct Capture {
    url: String,
    captured_at: u64,
    container: Container,
    detected_cards: usize,
    products: Vec<Product>,
}

/// Renders a capture as pretty-printed JSON, without touching the filesystem.
/// The disk write lives in [`write_capture`].
pub fn render(
    url: &str,
    captured_at: u64,
    detection: &Detection,
) -> Result<String, serde_json::Error> {
    let capture = Capture {
        url: url.to_string(),
        captured_at,
        container: detection.container.clone(),
        detected_cards: detection.products.len(),
        products: detection.products.clone(),
    };
    serde_json::to_string_pretty(&capture)
}

pub fn write_capture(dir: &str, url: &str, detection: &Detection) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let subdir = capture_dir(dir, url);
    let path = subdir.join(format!("capture-{now}.json"));
    let _ = fs::create_dir_all(&subdir);
    if let Ok(json) = render(url, now, detection) {
        let _ = fs::write(&path, json);
    }
    path
}

fn capture_dir(base: &str, url: &str) -> PathBuf {
    if let Some(host) = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
    {
        return PathBuf::from(base).join(host);
    }
    PathBuf::from(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_host_as_subfolder() {
        let dir = capture_dir(
            "captures",
            "https://perfumeriasfabilu.com.ar/categoria/perfumeria/",
        );
        assert_eq!(dir, PathBuf::from("captures/perfumeriasfabilu.com.ar"));
    }

    #[test]
    fn falls_back_to_base_dir_without_valid_host() {
        assert_eq!(
            capture_dir("captures", "not a url"),
            PathBuf::from("captures")
        );
        assert_eq!(capture_dir("captures", ""), PathBuf::from("captures"));
    }
}
