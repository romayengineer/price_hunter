use pocketbase_sdk::client::Client;
use price_hunter::detect::{Container, Detection, Product};
use price_hunter::store::Store;
use serde::Deserialize;

fn sample_detection() -> Detection {
    Detection {
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
                url: Some("/a/light-blue-homme-edp-50".to_string()),
                images: vec!["https://cdn.example/img/1.jpg".to_string()],
                currency: Some("ARS".to_string()),
            },
            Product {
                name: "212 Vip EDP 80".to_string(),
                price_text: "278.100".to_string(),
                price: 278100.0,
                url: Some("/b/212-vip-edp-80".to_string()),
                images: Vec::new(),
                currency: None,
            },
            Product {
                name: "A Drop d'Issey EDP Fraîche".to_string(),
                price_text: "143.000".to_string(),
                price: 143000.0,
                url: Some("/c/drop-d-issey-edp-fraiche".to_string()),
                images: Vec::new(),
                currency: Some("ARS".to_string()),
            },
        ],
    }
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct ProviderRow {
    id: String,
    domain: String,
    name: String,
    enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct ScrapeRow {
    id: String,
    url: String,
    product_count: usize,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct ProviderProductRow {
    id: String,
    provider_product_url: String,
    product_name: String,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct PriceRow {
    id: String,
    price: f64,
    currency: String,
    price_text: String,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct ImageRow {
    id: String,
    url: String,
    position: usize,
    is_primary: bool,
}

fn env_base_url() -> String {
    std::env::var("POCKETBASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8090".to_string())
}

fn env_password() -> String {
    std::env::var("POCKETBASE_SUPERUSER_PASSWORD")
        .unwrap_or_else(|_| "changeme".to_string())
}

/// Round-trips a detection through the PocketBase Record API against the
/// normalized schema (providers, scrapes, provider_products, provider_product_prices,
/// provider_product_images) and verifies the rows land. Requires a running PocketBase
/// (`pocketbase serve --dir ... --migrationsDir pocketbase/migrations`).
#[test]
#[ignore = "requires a running PocketBase instance"]
#[allow(clippy::cognitive_complexity)]
fn save_round_trips_through_the_api() {
    let url = "https://www.parfumerie.com.ar/fragancias";
    let host = "www.parfumerie.com.ar";
    let capture_path = format!("captures/{host}/capture-123456.json");
    let store = Store::connect().expect("connect to PocketBase");
    store
        .save(url, 123456, &capture_path, &sample_detection())
        .expect("save");

    let client = Client::new(&env_base_url())
        .superusers()
        .auth_with_password("admin@pricehunter.local", &env_password())
        .expect("admin auth");

    let providers = client
        .records("providers")
        .list()
        .filter(&format!("domain='{host}'"))
        .call::<ProviderRow>()
        .expect("list providers");
    assert_eq!(providers.items.len(), 1, "provider should be auto-created");

    let scrapes = client
        .records("scrapes")
        .list()
        .filter(&format!("url='{url}'"))
        .call::<ScrapeRow>()
        .expect("list scrapes");
    assert!(
        scrapes.items.iter().any(|s| s.product_count == 3),
        "scrape should have landed with product_count == 3, got {:?}",
        scrapes.items
    );

    let products = client
        .records("provider_products")
        .list()
        .call::<ProviderProductRow>()
        .expect("list provider products");
    let urls: Vec<&str> = products
        .items
        .iter()
        .map(|p| p.provider_product_url.as_str())
        .collect();
    assert!(
        urls.contains(&"/a/light-blue-homme-edp-50"),
        "provider product should be keyed by its URL, got {urls:?}"
    );
    assert!(
        products
            .items
            .iter()
            .any(|p| p.product_name == "A Drop d'Issey EDP Fraîche"),
        "an apostrophe in the product name must not break persistence, got {:?}",
        products.items
    );
    let pp_id = products
        .items
        .iter()
        .find(|p| p.provider_product_url == "/a/light-blue-homme-edp-50")
        .expect("provider product exists")
        .id
        .clone();

    let count_prices = || {
        client
            .records("provider_product_prices")
            .list()
            .filter(&format!("provider_product_id='{pp_id}'"))
            .call::<PriceRow>()
            .expect("list prices")
            .items
            .len()
    };

    let prices = client
        .records("provider_product_prices")
        .list()
        .filter(&format!("provider_product_id='{pp_id}'"))
        .call::<PriceRow>()
        .expect("list prices");
    assert!(
        prices
            .items
            .iter()
            .any(|p| p.price == 242100.0 && p.currency == "ARS" && p.price_text == "242.100"),
        "price with detected currency should land, got {:?}",
        prices.items
    );
    assert_eq!(count_prices(), 1, "first save records one price per product");

    store
        .save(url, 123457, &capture_path, &sample_detection())
        .expect("save unchanged");
    assert_eq!(
        count_prices(),
        1,
        "an unchanged price must not insert another row"
    );

    let mut changed = sample_detection();
    changed.products[0].price = 250000.0;
    changed.products[0].price_text = "250.000".to_string();
    store
        .save(url, 123458, &capture_path, &changed)
        .expect("save changed");
    assert_eq!(count_prices(), 2, "a price change inserts a new row");

    let mut currency_changed = sample_detection();
    currency_changed.products[0].currency = Some("USD".to_string());
    store
        .save(url, 123459, &capture_path, &currency_changed)
        .expect("save currency changed");
    assert_eq!(
        count_prices(),
        3,
        "a currency change with the same price inserts a new row"
    );

    let mut new_url = sample_detection();
    new_url.products[0].url = Some("/a/light-blue-homme-edp-50-v2".to_string());
    new_url.products[0].currency = Some("USD".to_string());
    store
        .save(url, 123460, &capture_path, &new_url)
        .expect("save same name at a new url");
    let named = client
        .records("provider_products")
        .list()
        .filter("provider_product_url='/a/light-blue-homme-edp-50-v2'")
        .call::<ProviderProductRow>()
        .expect("list provider products");
    assert!(
        named.items.is_empty(),
        "same product_name at a new url must reuse the existing row, got {:?}",
        named.items
    );
    assert_eq!(
        count_prices(),
        3,
        "reusing a row with an unchanged price adds no price row"
    );

    let images = client
        .records("provider_product_images")
        .list()
        .call::<ImageRow>()
        .expect("list product images");
    assert!(
        images.items.iter().any(|i| i.url == "https://cdn.example/img/1.jpg" && i.is_primary),
        "primary image should land, got {:?}",
        images.items
    );
    assert!(
        images.items.iter().all(|i| i.url == "https://cdn.example/img/1.jpg"),
        "image rows should be keyed by url (no duplicates), got {:?}",
        images.items
    );
}
