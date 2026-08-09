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
            },
            Product {
                name: "212 Vip EDP 80".to_string(),
                price_text: "278.100".to_string(),
                price: 278100.0,
            },
        ],
    }
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct ProductRow {
    id: String,
    name: String,
    price_text: String,
    price: f64,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct CaptureRow {
    id: String,
    url: String,
    host: String,
    detected_cards: usize,
}

fn env_base_url() -> String {
    std::env::var("POCKETBASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8090".to_string())
}

fn env_password() -> String {
    std::env::var("POCKETBASE_SUPERUSER_PASSWORD")
        .unwrap_or_else(|_| "changeme".to_string())
}

/// Round-trips a capture + products through the PocketBase Record API and
/// verifies the rows land in the collections. Requires a running PocketBase
/// (`pocketbase serve --dir ... --migrationsDir pocketbase/migrations`).
#[test]
#[ignore = "requires a running PocketBase instance"]
#[allow(clippy::cognitive_complexity)]
fn save_round_trips_through_the_api() {
    let url = "https://www.parfumerie.com.ar/fragancias";
    let store = Store::connect().expect("connect to PocketBase");
    store.save(url, 123456, &sample_detection()).expect("save");

    let client = Client::new(&env_base_url())
        .superusers()
        .auth_with_password("admin@pricehunter.local", &env_password())
        .expect("admin auth");

    let captures = client
        .records("captures")
        .list()
        .filter(&format!("url='{url}'"))
        .call::<CaptureRow>()
        .expect("list captures");
    assert!(
        captures.items.iter().any(|c| c.host == "www.parfumerie.com.ar" && c.detected_cards == 2),
        "capture should have landed with host + detected_cards, got {:?}",
        captures.items
    );

    let products = client
        .records("products")
        .list()
        .call::<ProductRow>()
        .expect("list products");
    let names: Vec<&str> = products.items.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"Light Blue Homme EDP 50"), "missing first product");
    assert!(names.contains(&"212 Vip EDP 80"), "missing second product");

    let found = products
        .items
        .iter()
        .find(|p| p.name == "Light Blue Homme EDP 50")
        .expect("first product present");
    assert_eq!(found.price, 242100.0);
    assert_eq!(found.price_text, "242.100");
}
