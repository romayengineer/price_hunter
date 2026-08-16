mod common;

use price_hunter::detect::{Product, detect_grid};

const BEAUTY24_URL: &str = "https://www.beauty24.com.ar/perfumes-y-fragancias/";

fn products() -> Vec<Product> {
    vec![
        Product {
            name: String::from("Dylan Blush Pink EDP 100 ml + Neceser"),
            price_text: String::new(),
            price: 328000.0,
            ..Default::default()
        },
        Product {
            name: String::from("Crystal Emerald EDP 90 ml"),
            price_text: String::new(),
            price: 328000.0,
            ..Default::default()
        },
        Product {
            name: String::from("Blue Jeans EDT 75 ml"),
            price_text: String::new(),
            price: 79990.0,
            ..Default::default()
        },
        Product {
            name: String::from("Fresh Gold EDP 100 ml"),
            price_text: String::new(),
            price: 95400.0,
            ..Default::default()
        },
        Product {
            name: String::from("Funny EDT Ed. Limitada 100ml"),
            price_text: String::new(),
            price: 94340.0,
            ..Default::default()
        },
    ]
}

#[test]
fn extracts_all_prices_from_beauty24_fixture() {
    common::assert_fixture(
        "tests/fixtures/beauty24.html",
        &products(),
        "vtex-search-result-3-x-gallery",
    );
}

#[test]
#[ignore = "requires network access"]
fn extracts_all_prices_from_beauty24_live() {
    let html = ureq::get(BEAUTY24_URL)
        .call()
        .expect("failed to fetch page")
        .into_string()
        .expect("invalid UTF-8 body");
    let detection = detect_grid(&html).expect("grid should be detected");
    assert!(
        detection.products.len() >= 12,
        "expected at least 12 products"
    );
    assert!(detection.products.iter().all(|p| p.price > 0.0));
}
