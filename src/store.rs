use anyhow::{Context, Result};
use pocketbase_sdk::client::{Auth, Client};
use serde::{Deserialize, Serialize};

use crate::detect::Detection;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8090";
const PROVIDERS_COLLECTION: &str = "providers";
const SCRAPES_COLLECTION: &str = "scrapes";
const PROVIDER_PRODUCTS_COLLECTION: &str = "provider_products";
const PROVIDER_PRODUCT_IMAGES_COLLECTION: &str = "provider_product_images";
const PROVIDER_PRODUCT_PRICES_COLLECTION: &str = "provider_product_prices";

/// Payload for the `providers` collection.
#[derive(Serialize, Clone)]
struct ProviderPayload {
    domain: String,
    name: String,
    enabled: bool,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
struct ProviderRow {
    id: String,
    domain: String,
    name: String,
    enabled: bool,
    default_currency: Option<String>,
}

/// Payload for the `scrapes` collection.
#[derive(Serialize, Clone)]
struct ScrapePayload {
    provider_id: String,
    url: String,
    scraped_at: String,
    status: String,
    capture_path: String,
    product_count: usize,
    container_class: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct ScrapeRow {
    id: String,
}

/// Payload for the `provider_products` collection.
#[derive(Serialize, Clone)]
struct ProviderProductPayload {
    provider_id: String,
    provider_product_url: String,
    product_name: String,
    last_seen_at: String,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
struct ProviderProductRow {
    id: String,
}

/// Payload for the `provider_product_images` collection.
#[derive(Serialize, Clone)]
struct ProductImagePayload {
    provider_product_id: String,
    url: String,
    position: usize,
    is_primary: bool,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
struct ProductImageRow {
    id: String,
    url: String,
    position: usize,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
struct PriceRow {
    id: String,
    price: f64,
    currency: String,
}

/// Payload for the `provider_product_prices` collection.
#[derive(Serialize, Clone)]
struct ProviderPricePayload {
    provider_product_id: String,
    scrape_id: String,
    price: f64,
    currency: String,
    price_text: String,
}

/// Persists detections to a running PocketBase through its Record API using the
/// normalized schema documented in DATABASE.md (providers, scrapes,
/// provider_products, provider_product_images, provider_product_prices).
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

    /// Persists one detection through the Record API:
    /// one `scrapes` record, then per detected product one `provider_products`
    /// (upserted by `(provider_id, provider_product_url)`), a
    /// `provider_product_prices` record only when the price changed, and its
    /// `provider_product_images` rows.
    pub fn save(
        &self,
        url: &str,
        captured_at: u64,
        capture_path: &str,
        detection: &Detection,
    ) -> Result<()> {
        let host = host_of(url);
        let provider = self.ensure_provider(&host)?;
        let scrape = self.create_scrape(url, captured_at, capture_path, &provider.id, detection)?;
        for product in &detection.products {
            let provider_product = self.ensure_provider_product(
                &provider,
                product.url.as_deref().unwrap_or(""),
                &product.name,
            )?;
            let currency = product
                .currency
                .clone()
                .or_else(|| provider.default_currency.clone())
                .unwrap_or_default();
            self.create_price(&provider_product.id, &scrape.id, currency, product)?;
            self.sync_images(&provider_product.id, &product.images)?;
        }
        Ok(())
    }

    /// Returns the existing provider for `domain` or creates it (name = domain,
    /// enabled = true).
    fn ensure_provider(&self, domain: &str) -> Result<ProviderRow> {
        let existing = self
            .client
            .records(PROVIDERS_COLLECTION)
            .list()
            .filter(&format!("domain='{domain}'"))
            .per_page(1)
            .call::<ProviderRow>()
            .context("could not look up provider")?;
        if let Some(row) = existing.items.into_iter().next() {
            return Ok(row);
        }
        let created = self
            .client
            .records(PROVIDERS_COLLECTION)
            .create(ProviderPayload {
                domain: domain.to_string(),
                name: domain.to_string(),
                enabled: true,
            })
            .call()
            .map_err(|e| anyhow::anyhow!("could not create provider: {e}"))?;
        Ok(ProviderRow {
            id: created.id,
            domain: domain.to_string(),
            name: domain.to_string(),
            enabled: true,
            default_currency: None,
        })
    }

    fn create_scrape(
        &self,
        url: &str,
        captured_at: u64,
        capture_path: &str,
        provider_id: &str,
        detection: &Detection,
    ) -> Result<ScrapeRow> {
        let container_class = detection.container.classes.first().cloned().unwrap_or_default();
        self.client
            .records(SCRAPES_COLLECTION)
            .create(ScrapePayload {
                provider_id: provider_id.to_string(),
                url: url.to_string(),
                scraped_at: iso8601(captured_at),
                status: "success".to_string(),
                capture_path: capture_path.to_string(),
                product_count: detection.products.len(),
                container_class,
            })
            .call()
            .map_err(|e| anyhow::anyhow!("could not create scrape: {e}"))
            .map(|r| ScrapeRow { id: r.id })
    }

    /// Returns the provider product for `(provider_id, provider_product_url)`,
    /// creating it (with `product_name` and `last_seen_at` set) when it does
    /// not exist yet.
    fn ensure_provider_product(
        &self,
        provider: &ProviderRow,
        provider_product_url: &str,
        product_name: &str,
    ) -> Result<ProviderProductRow> {
        let filter = format!(
            "provider_id='{}' && provider_product_url='{}'",
            provider.id, provider_product_url
        );
        let existing = self
            .client
            .records(PROVIDER_PRODUCTS_COLLECTION)
            .list()
            .filter(&filter)
            .per_page(1)
            .call::<ProviderProductRow>()
            .context("could not look up provider product")?;
        if let Some(row) = existing.items.into_iter().next() {
            return Ok(row);
        }
        let created = self
            .client
            .records(PROVIDER_PRODUCTS_COLLECTION)
            .create(ProviderProductPayload {
                provider_id: provider.id.clone(),
                provider_product_url: provider_product_url.to_string(),
                product_name: product_name.to_string(),
                last_seen_at: iso8601(now_secs()),
            })
            .call()
            .map_err(|e| anyhow::anyhow!("could not create provider product: {e}"))?;
        Ok(ProviderProductRow { id: created.id })
    }

    /// Inserts a price row only when it differs from the last recorded price
    /// for this provider product. The first observation is always recorded.
    /// A row is written when `price` or `currency` changed.
    fn create_price(
        &self,
        provider_product_id: &str,
        scrape_id: &str,
        currency: String,
        product: &crate::detect::Product,
    ) -> Result<()> {
        let last = self
            .client
            .records(PROVIDER_PRODUCT_PRICES_COLLECTION)
            .list()
            .filter(&format!("provider_product_id='{provider_product_id}'"))
            .sort("-created")
            .per_page(1)
            .call::<PriceRow>()
            .context("could not look up last price")?;
        if matches!(
            last.items.into_iter().next(),
            Some(row) if row.price == product.price && row.currency == currency
        ) {
            return Ok(());
        }
        self.client
            .records(PROVIDER_PRODUCT_PRICES_COLLECTION)
            .create(ProviderPricePayload {
                provider_product_id: provider_product_id.to_string(),
                scrape_id: scrape_id.to_string(),
                price: product.price,
                currency,
                price_text: product.price_text.clone(),
            })
            .call()
            .map_err(|e| anyhow::anyhow!("could not create price: {e}"))
            .map(|_| ())
    }

    /// Upserts the product images keyed by url and removes rows that are no
    /// longer present. Position 0 is marked as the primary image.
    fn sync_images(&self, provider_product_id: &str, images: &[String]) -> Result<()> {
        let existing = self
            .client
            .records(PROVIDER_PRODUCT_IMAGES_COLLECTION)
            .list()
            .filter(&format!("provider_product_id='{provider_product_id}'"))
            .per_page(100)
            .call::<ProductImageRow>()
            .context("could not look up product images")?;

        for (position, url) in images.iter().enumerate() {
            self.upsert_image(provider_product_id, position, url, &existing.items)?;
        }
        self.remove_stale_images(&existing.items, images)?;
        Ok(())
    }

    fn upsert_image(
        &self,
        provider_product_id: &str,
        position: usize,
        url: &str,
        existing: &[ProductImageRow],
    ) -> Result<()> {
        let payload = ProductImagePayload {
            provider_product_id: provider_product_id.to_string(),
            url: url.to_string(),
            position,
            is_primary: position == 0,
        };
        match existing.iter().find(|row| row.url == url) {
            Some(row) => self
                .client
                .records(PROVIDER_PRODUCT_IMAGES_COLLECTION)
                .update(&row.id, payload)
                .call()
                .map(|_| ()),
            None => self
                .client
                .records(PROVIDER_PRODUCT_IMAGES_COLLECTION)
                .create(payload)
                .call()
                .map(|_| ()),
        }
        .map_err(|e| anyhow::anyhow!("could not write product image: {e}"))
    }

    fn remove_stale_images(&self, existing: &[ProductImageRow], images: &[String]) -> Result<()> {
        for row in existing {
            if !images.contains(&row.url) {
                self.client
                    .records(PROVIDER_PRODUCT_IMAGES_COLLECTION)
                    .destroy(&row.id)
                    .call()
                    .map_err(|e| anyhow::anyhow!("could not delete stale product image: {e}"))?;
            }
        }
        Ok(())
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Formats a unix-seconds timestamp as the ISO-8601 string PocketBase expects
/// for `date` fields (`YYYY-MM-DD HH:MM:SS.mmmZ`, UTC). The store never sends
/// raw epoch numbers — PocketBase treats them as blank.
fn iso8601(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}.000Z")
}

/// Gregorian calendar day to (year, month, day) from the Unix epoch in days
/// (Howard Hinnant's `civil_from_days` algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
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
    fn iso8601_formats_utc_datetime() {
        assert_eq!(iso8601(0), "1970-01-01 00:00:00.000Z");
        assert_eq!(iso8601(1_234_567_890), "2009-02-13 23:31:30.000Z");
        assert_eq!(iso8601(123456), "1970-01-02 10:17:36.000Z");
    }

    #[test]
    fn scrape_payload_serializes_detection_fields() {
        let payload = ScrapePayload {
            provider_id: "prov-1".to_string(),
            url: "https://www.parfumerie.com.ar/fragancias".to_string(),
            scraped_at: iso8601(123456),
            status: "success".to_string(),
            capture_path: "captures/www.parfumerie.com.ar/capture-123456.json".to_string(),
            product_count: 2,
            container_class: "products".to_string(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["provider_id"], "prov-1");
        assert_eq!(json["url"], "https://www.parfumerie.com.ar/fragancias");
        assert_eq!(json["scraped_at"], "1970-01-02 10:17:36.000Z");
        assert_eq!(json["status"], "success");
        assert_eq!(
            json["capture_path"],
            "captures/www.parfumerie.com.ar/capture-123456.json"
        );
        assert_eq!(json["product_count"], 2);
        assert_eq!(json["container_class"], "products");
    }

    #[test]
    fn provider_product_payload_serializes_relation_and_key() {
        let payload = ProviderProductPayload {
            provider_id: "prov-1".to_string(),
            provider_product_url: "/a/light-blue-homme-edp-50".to_string(),
            product_name: String::new(),
            last_seen_at: iso8601(123456),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["provider_id"], "prov-1");
        assert_eq!(
            json["provider_product_url"],
            "/a/light-blue-homme-edp-50"
        );
        assert_eq!(json["last_seen_at"], "1970-01-02 10:17:36.000Z");
    }

    #[test]
    fn price_payload_serializes_relation_and_values() {
        let payload = ProviderPricePayload {
            provider_product_id: "pp-1".to_string(),
            scrape_id: "scr-1".to_string(),
            price: 242100.0,
            currency: "ARS".to_string(),
            price_text: "242.100".to_string(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["provider_product_id"], "pp-1");
        assert_eq!(json["scrape_id"], "scr-1");
        assert_eq!(json["price"], 242100.0);
        assert_eq!(json["currency"], "ARS");
        assert_eq!(json["price_text"], "242.100");
    }

    #[test]
    fn product_image_payload_marks_first_as_primary() {
        let payload = ProductImagePayload {
            provider_product_id: "pp-1".to_string(),
            url: "https://cdn.example/img/1.jpg".to_string(),
            position: 0,
            is_primary: true,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["provider_product_id"], "pp-1");
        assert_eq!(json["position"], 0);
        assert_eq!(json["is_primary"], true);
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
                    ..Product::default()
                },
                Product {
                    name: "212 Vip EDP 80".to_string(),
                    price_text: "278.100".to_string(),
                    price: 278100.0,
                    ..Product::default()
                },
            ],
        };
        assert_eq!(detection.products.len(), 2);
        assert_eq!(detection.container.child_count, 2);
    }
}
