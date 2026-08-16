use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub(super) const PRODUCTS_COLLECTION: &str = "products";
pub(super) const BRANDS_COLLECTION: &str = "brand";
pub(super) const PROVIDERS_COLLECTION: &str = "providers";
pub(super) const SCRAPES_COLLECTION: &str = "scrapes";
pub(super) const PROVIDER_PRODUCTS_COLLECTION: &str = "provider_products";
pub(super) const PROVIDER_PRODUCT_IMAGES_COLLECTION: &str = "provider_product_images";
pub(super) const PROVIDER_PRODUCT_MATCHES_COLLECTION: &str = "provider_product_matches";
pub(super) const PROVIDER_PRODUCT_PRICES_COLLECTION: &str = "provider_product_prices";

/// Payload for the `products` collection.
#[derive(Serialize, Clone)]
pub(super) struct ProductImportPayload {
    pub(super) brand: String,
    pub(super) product_name: String,
    pub(super) name: String,
    pub(super) size: String,
    pub(super) category: String,
    pub(super) active: bool,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
pub(super) struct ProductImportRow {
    pub(super) id: String,
}

/// Result of importing a single CSV row.
pub(super) enum RowOutcome {
    Created,
    Skipped,
}

/// Payload for the `brand` collection.
#[derive(Serialize, Clone)]
pub(super) struct BrandPayload {
    pub(super) name: String,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
pub(super) struct BrandRow {
    pub(super) id: String,
    pub(super) name: String,
}

/// Payload for the `providers` collection.
#[derive(Serialize, Clone)]
pub(super) struct ProviderPayload {
    pub(super) domain: String,
    pub(super) name: String,
    pub(super) enabled: bool,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
pub(super) struct ProviderRow {
    pub(super) id: String,
    pub(super) domain: String,
    pub(super) name: String,
    pub(super) enabled: bool,
    pub(super) default_currency: Option<String>,
}

/// Payload for the `scrapes` collection.
#[derive(Serialize, Clone)]
pub(super) struct ScrapePayload {
    pub(super) provider_id: String,
    pub(super) url: String,
    pub(super) scraped_at: String,
    pub(super) status: String,
    pub(super) capture_path: String,
    pub(super) product_count: usize,
    pub(super) container_class: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub(super) struct ScrapeRow {
    pub(super) id: String,
}

/// Payload for the `provider_products` collection.
#[derive(Serialize, Clone)]
pub(super) struct ProviderProductPayload {
    pub(super) provider_id: String,
    pub(super) provider_product_url: String,
    pub(super) name: String,
    pub(super) last_seen_at: String,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
pub(super) struct ProviderProductRow {
    pub(super) id: String,
    pub(super) provider_id: String,
    pub(super) name: String,
    pub(super) product_id: Option<String>,
    pub(super) brand_id: Option<String>,
}

/// A canonical product used by the fuzzy matcher. `name` already holds the
/// full display name (brand + product_name + size); `brand` is the canonical
/// brand (also used to assign a brand to linked provider products).
#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
pub(super) struct ProductRow {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) brand: String,
}

/// Payload for updating `provider_products.product_id`. A `None` value
/// serializes as `null`, clearing the relation.
#[derive(Serialize, Clone)]
pub(super) struct ProductLinkPayload {
    pub(super) product_id: Option<String>,
}

/// Payload for updating `provider_products.brand_id`. A `None` value
/// serializes as `null`, clearing the brand assignment.
#[derive(Serialize, Clone)]
pub(super) struct BrandLinkPayload {
    pub(super) brand_id: Option<String>,
}

/// A `provider_product_prices` row used to resolve the latest price per
/// provider product (list sorted by `-created`, keep first occurrence).
#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
pub(super) struct ProviderPriceRow {
    pub(super) id: String,
    pub(super) provider_product_id: String,
    pub(super) price: f64,
}

/// One provider column in the product × provider matrix.
#[derive(Serialize)]
pub struct MatrixProvider {
    pub id: String,
    pub domain: String,
    pub name: String,
}

/// One product row in the matrix: the full display name (brand, product_name
/// and size joined) plus the latest price per provider id. Providers that
/// don't carry the product are simply absent from `prices`.
#[derive(Serialize)]
pub struct MatrixRow {
    pub product_id: String,
    pub name: String,
    pub prices: HashMap<String, f64>,
}

/// The product × provider price matrix served by `GET /matrix`. Every row has
/// at least one linked provider product (no all-blank rows); columns include
/// every provider.
#[derive(Serialize)]
pub struct Matrix {
    pub generated_at: String,
    pub providers: Vec<MatrixProvider>,
    pub rows: Vec<MatrixRow>,
}

/// Payload for the `provider_product_matches` collection.
#[derive(Serialize, Clone)]
pub(super) struct ProviderMatchPayload {
    pub(super) provider_product_id: String,
    pub(super) product_id: String,
    pub(super) score: f64,
    pub(super) status: String,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
pub(super) struct ProviderMatchRow {
    pub(super) id: String,
    pub(super) provider_product_id: String,
    pub(super) product_id: String,
    pub(super) score: f64,
    pub(super) status: String,
}

/// Page shape of a `provider_product_matches` list response.
#[derive(Deserialize)]
pub(super) struct MatchListResponse {
    pub(super) items: Vec<ProviderMatchRow>,
}

/// Outcome of writing one comparison row.
pub(super) enum MatchInsert {
    /// The row was created.
    Created,
    /// The pair already exists (unique index) — e.g. inserted by a concurrent
    /// run — so it counts as already computed.
    AlreadyExists,
}

/// Payload for the `provider_product_images` collection.
#[derive(Serialize, Clone)]
pub(super) struct ProductImagePayload {
    pub(super) provider_product_id: String,
    pub(super) url: String,
    pub(super) position: usize,
    pub(super) is_primary: bool,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
pub(super) struct ProductImageRow {
    pub(super) id: String,
    pub(super) url: String,
    pub(super) position: usize,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
pub(super) struct PriceRow {
    pub(super) id: String,
    pub(super) price: f64,
    pub(super) currency: String,
}

/// Payload for the `provider_product_prices` collection.
#[derive(Serialize, Clone)]
pub(super) struct ProviderPricePayload {
    pub(super) provider_product_id: String,
    pub(super) scrape_id: String,
    pub(super) price: f64,
    pub(super) currency: String,
    pub(super) price_text: String,
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;
    use crate::store::http::iso8601;

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
            name: String::new(),
            last_seen_at: iso8601(123456),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["provider_id"], "prov-1");
        assert_eq!(
            json["provider_product_url"],
            "/a/light-blue-homme-edp-50"
        );
        assert_eq!(json["name"], "");
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
    fn match_payload_serializes_relations_score_and_status() {
        let payload = ProviderMatchPayload {
            provider_product_id: "pp-1".to_string(),
            product_id: "prod-1".to_string(),
            score: 0.87,
            status: "confirmed".to_string(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["provider_product_id"], "pp-1");
        assert_eq!(json["product_id"], "prod-1");
        assert_eq!(json["score"], 0.87);
        assert_eq!(json["status"], "confirmed");
    }

    #[test]
    fn product_link_payload_serializes_some_as_id_and_none_as_null() {
        let linked = serde_json::to_value(ProductLinkPayload {
            product_id: Some("prod-1".to_string()),
        })
        .unwrap();
        assert_eq!(linked["product_id"], "prod-1");

        let cleared = serde_json::to_value(ProductLinkPayload { product_id: None }).unwrap();
        assert_eq!(cleared["product_id"], serde_json::Value::Null);
    }

    #[test]
    fn product_import_payload_serializes_csv_columns_and_active() {
        let payload = ProductImportPayload {
            brand: "adolfo dominguez".to_string(),
            product_name: "adn neroli ecstasy".to_string(),
            name: "adolfo dominguez adn neroli ecstasy 100 ml".to_string(),
            size: "100 ml".to_string(),
            category: String::new(),
            active: true,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["brand"], "adolfo dominguez");
        assert_eq!(json["product_name"], "adn neroli ecstasy");
        assert_eq!(json["name"], "adolfo dominguez adn neroli ecstasy 100 ml");
        assert_eq!(json["size"], "100 ml");
        assert_eq!(json["category"], "");
        assert_eq!(json["active"], true);
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
}
