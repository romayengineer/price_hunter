//! Writes detections under `captures/<host>/`. Rendering and path logic
//! live in `price_hunter_domain::capture`; this module owns the filesystem.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use price_hunter_domain::model::Detection;

pub use price_hunter_domain::capture::{build_capture_delta, build_delta_detection, render};

/// Renders a capture as pretty-printed JSON and writes it under
/// `dir/<host>/capture-<timestamp>.json` (falls back to `dir/` when the URL
/// has no host). Returns the path written.
pub fn write_capture(dir: &str, url: &str, detection: &Detection) -> std::io::Result<PathBuf> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let subdir = price_hunter_domain::capture::capture_dir(dir, url);
    let path = subdir.join(format!("capture-{now}.json"));
    fs::create_dir_all(&subdir)?;
    let json = render(url, now, detection).map_err(std::io::Error::other)?;
    fs::write(&path, json)?;
    Ok(path)
}
