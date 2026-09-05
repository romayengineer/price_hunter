//! Pure capture rendering: JSON text and capture paths/deltas. No
//! filesystem writes — infrastructure owns the I/O.

use std::collections::HashSet;
use std::path::PathBuf;

use serde::Serialize;

use crate::model::{Container, Detection, Product};

#[derive(Serialize)]
struct Capture {
    url: String,
    captured_at: u64,
    container: Container,
    detected_cards: usize,
    products: Vec<Product>,
}

/// Renders a capture as pretty-printed JSON, without touching the filesystem.
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

/// The directory a capture for `url` belongs under: `base/<host>/`
/// (falls back to `base/` when the URL has no host).
pub fn capture_dir(base: &str, url: &str) -> PathBuf {
    let host = crate::net::host_of(url);
    if host.is_empty() {
        PathBuf::from(base)
    } else {
        PathBuf::from(base).join(host)
    }
}

/// The number of products in an optional detection (0 when none).
pub fn count_of(detection: &Option<Detection>) -> usize {
    detection.as_ref().map_or(0, |d| d.products.len())
}

/// Builds a synthetic [`Detection`] holding only the newly-seen products
/// (empty container classes, child count = product count).
pub fn build_delta_detection(new_products: &[Product]) -> Detection {
    Detection {
        container: Container {
            classes: Vec::new(),
            id: None,
            child_count: new_products.len(),
        },
        products: new_products.to_vec(),
    }
}

/// Builds a [`Detection`] holding only the delta products, keeping the
/// original container but with the child count set to the delta length.
pub fn build_capture_delta(detection: &Detection, delta: &[Product]) -> Detection {
    let mut container = detection.container.clone();
    container.child_count = delta.len();
    Detection {
        container,
        products: delta.to_vec(),
    }
}

/// The first CSS class of a grid container (empty when the container has no
/// classes). Persisted as `scrapes.container_class`.
pub fn container_class(classes: &[String]) -> String {
    classes.first().cloned().unwrap_or_default()
}

/// Whether `next` page source differs from the last polled one (`None` means
/// first poll — always changed).
pub fn source_changed(last_source: Option<&str>, next: &str) -> bool {
    last_source != Some(next)
}

/// Removes every `delta` product key from `seen`, so a failed capture write
/// can be retried on the next poll instead of silently dropping products.
pub fn rollback_seen(seen: &mut HashSet<String>, delta: &[Product]) {
    for product in delta {
        seen.remove(&product.delta_key());
    }
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

    #[test]
    fn render_includes_url_timestamp_and_products() {
        let detection = Detection {
            container: Container {
                classes: vec!["grid".to_string()],
                id: None,
                child_count: 1,
            },
            products: vec![Product {
                name: "Alpha EDP 50 ml".to_string(),
                price_text: "100".to_string(),
                price: 100.0,
                ..Product::default()
            }],
        };
        let json = render("https://example.com/list", 123, &detection).expect("renders");
        assert!(json.contains("https://example.com/list"));
        assert!(json.contains("Alpha EDP 50 ml"));
        assert!(json.contains("\"detected_cards\": 1"));
    }

    #[test]
    fn count_of_counts_optional_detection() {
        assert_eq!(count_of(&None), 0);
        let products = vec![Product {
            name: "A".to_string(),
            ..Product::default()
        }];
        let delta = build_delta_detection(&products);
        assert_eq!(count_of(&Some(delta)), 1);
    }

    #[test]
    fn delta_detections_carry_product_counts() {
        let products = vec![Product {
            name: "A".to_string(),
            ..Product::default()
        }];
        let delta = build_delta_detection(&products);
        assert_eq!(delta.products.len(), 1);
        assert_eq!(delta.container.child_count, 1);
        let capped = build_capture_delta(&delta, &products[..0]);
        assert_eq!(capped.container.child_count, 0);
        assert!(capped.products.is_empty());
    }
}
