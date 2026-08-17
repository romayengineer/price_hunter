use crate::common;
use price_hunter::detect::Product;

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
