use anyhow::{Context, Result};
use pocketbase_sdk::client::{Auth, Client};
use serde::Serialize;

use crate::detect::Detection;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8090";
const CAPTURES_COLLECTION: &str = "captures";
const PRODUCTS_COLLECTION: &str = "products";

/// Payload for the `captures` collection.
#[derive(Serialize, Clone)]
struct CapturePayload {
    url: String,
    host: String,
    captured_at: u64,
    container_classes: String,
    container_id: Option<String>,
    child_count: usize,
    detected_cards: usize,
}

/// Payload for the `products` collection. `capture` is the related captures id.
#[derive(Serialize, Clone)]
struct ProductPayload {
    capture: String,
    name: String,
    price_text: String,
    price: f64,
}

/// Persists captures to a running PocketBase through its Record API.
///
/// The scraper NEVER writes SQL or touches the database file directly — all
/// writes go through the PocketBase HTTP API as an authenticated superuser
/// (superusers bypass collection rules, so no app user is required).
pub struct Store {
    client: Client<Auth>,
}

impl Store {
    /// Authenticates against a running PocketBase instance.
    ///
    /// Reads `POCKETBASE_URL` (default `http://127.0.0.1:8090`),
    /// `POCKETBASE_SUPERUSER_EMAIL` (default `admin@pricehunter.local`) and
    /// `POCKETBASE_SUPERUSER_PASSWORD` (required).
    pub fn connect() -> Result<Self> {
        let base_url =
            std::env::var("POCKETBASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let email = std::env::var("POCKETBASE_SUPERUSER_EMAIL")
            .unwrap_or_else(|_| "admin@pricehunter.local".to_string());
        let password = std::env::var("POCKETBASE_SUPERUSER_PASSWORD")
            .context("POCKETBASE_SUPERUSER_PASSWORD is not set")?;
        let client = Client::new(&base_url)
            .superusers()
            .auth_with_password(&email, &password)
            .map_err(|e| {
                anyhow::anyhow!("could not authenticate to PocketBase at {base_url}: {e}")
            })?;
        Ok(Self { client })
    }

    /// Persists one detection (a capture + its products) through the Record API:
    /// one `captures` record, then one `products` record per detected product
    /// linked to the capture via the `capture` relation.
    pub fn save(&self, url: &str, captured_at: u64, detection: &Detection) -> Result<()> {
        let host = host_of(url);
        let classes = serde_json::to_string(&detection.container.classes).unwrap_or_default();
        let capture = self
            .client
            .records(CAPTURES_COLLECTION)
            .create(CapturePayload {
                url: url.to_string(),
                host,
                captured_at,
                container_classes: classes,
                container_id: detection.container.id.clone(),
                child_count: detection.container.child_count,
                detected_cards: detection.products.len(),
            })
            .call()
            .map_err(|e| anyhow::anyhow!("could not create capture: {e}"))?;
        for product in &detection.products {
            self.client
                .records(PRODUCTS_COLLECTION)
                .create(ProductPayload {
                    capture: capture.id.clone(),
                    name: product.name.clone(),
                    price_text: product.price_text.clone(),
                    price: product.price,
                })
                .call()
                .map_err(|e| anyhow::anyhow!("could not create product: {e}"))?;
        }
        Ok(())
    }
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;
    use crate::detect::{Container, Product};

    #[test]
    fn host_of_extracts_host() {
        assert_eq!(
            host_of("https://www.parfumerie.com.ar/fragancias"),
            "www.parfumerie.com.ar"
        );
        assert_eq!(host_of("not a url"), "");
    }

    #[test]
    fn capture_payload_serializes_detection_fields() {
        let payload = CapturePayload {
            url: "https://www.parfumerie.com.ar/fragancias".to_string(),
            host: "www.parfumerie.com.ar".to_string(),
            captured_at: 123456,
            container_classes: r#"["products","row"]"#.to_string(),
            container_id: Some("grid-1".to_string()),
            child_count: 2,
            detected_cards: 2,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["url"], "https://www.parfumerie.com.ar/fragancias");
        assert_eq!(json["host"], "www.parfumerie.com.ar");
        assert_eq!(json["captured_at"], 123456);
        assert_eq!(json["container_classes"], r#"["products","row"]"#);
        assert_eq!(json["container_id"], "grid-1");
        assert_eq!(json["child_count"], 2);
        assert_eq!(json["detected_cards"], 2);
    }

    #[test]
    fn product_payload_serializes_relation_and_price() {
        let payload = ProductPayload {
            capture: "capture-id".to_string(),
            name: "Light Blue Homme EDP 50".to_string(),
            price_text: "242.100".to_string(),
            price: 242100.0,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["capture"], "capture-id");
        assert_eq!(json["name"], "Light Blue Homme EDP 50");
        assert_eq!(json["price_text"], "242.100");
        assert_eq!(json["price"], 242100.0);
    }

    #[test]
    fn sample_detection_builds_one_capture_and_two_products() {
        let detection = Detection {
            container: Container {
                classes: vec!["products".to_string(), "row".to_string()],
                id: Some("grid-1".to_string()),
                child_count: 2,
            },
            products: vec![
                Product {
                    name: "Light Blue Homme EDP 50".to_string(),
                    price_text: "242.100".to_string(),
                    price: 242100.0,
                },
                Product {
                    name: "212 Vip EDP 80".to_string(),
                    price_text: "278.100".to_string(),
                    price: 278100.0,
                },
            ],
        };
        assert_eq!(detection.products.len(), 2);
        assert_eq!(detection.container.child_count, 2);
    }
}
