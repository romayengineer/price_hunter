mod common;

use price_hunter::detect::{detect_grid, Product};

const FABILU_URL: &str = "https://perfumeriasfabilu.com.ar/categoria/perfumeria/";

fn products() -> Vec<Product> {
    vec![
        Product {
            name: String::from("PAULVIC WOMAN X50ML"),
            price_text: String::new(),
            price: 8190.0,
            ..Default::default()
        },
        Product {
            name: String::from("PAULVIC MEN X50ML"),
            price_text: String::new(),
            price: 8190.0,
            ..Default::default()
        },
        Product {
            name: String::from("PIBES COLONIA X95ML"),
            price_text: String::new(),
            price: 7400.0,
            ..Default::default()
        },
        Product {
            name: String::from("MUJERCITAS EDP X 40 ML"),
            price_text: String::new(),
            price: 7020.0,
            ..Default::default()
        },
        Product {
            name: String::from("DANIELLE EDT X90ML"),
            price_text: String::new(),
            price: 7117.0,
            ..Default::default()
        },
    ]
}

#[test]
fn extracts_all_prices_from_fabilu_fixture() {
    common::assert_fixture("tests/fixtures/fabilu.html", &products(), "products");
}

#[test]
#[ignore = "requires network access"]
fn extracts_all_prices_from_fabilu_live() {
    let html = ureq::get(FABILU_URL)
        .call()
        .expect("failed to fetch page")
        .into_string()
        .expect("invalid UTF-8 body");
    let detection = detect_grid(&html).expect("grid should be detected");
    assert!(detection.products.len() >= 12, "expected at least 12 products");
    assert!(detection.products.iter().all(|p| p.price > 0.0));
}
