use crate::common;
use price_hunter::detect::Product;

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
