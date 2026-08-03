use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::detect::{Container, Detection, Product};

#[derive(Serialize)]
struct Capture {
    url: String,
    captured_at: u64,
    container: Container,
    detected_cards: usize,
    products: Vec<Product>,
}

pub fn write_capture(dir: &str, url: &str, detection: &Detection) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let path = PathBuf::from(dir).join(format!("capture-{now}.json"));
    let capture = Capture {
        url: url.to_string(),
        captured_at: now,
        container: detection.container.clone(),
        detected_cards: detection.products.len(),
        products: detection.products.clone(),
    };
    let _ = fs::create_dir_all(dir);
    if let Ok(json) = serde_json::to_string_pretty(&capture) {
        let _ = fs::write(&path, json);
    }
    path
}
