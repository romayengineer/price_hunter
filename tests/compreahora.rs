mod common;

use price_hunter::detect::Product;

// prices are under class mobile-product-price-price-number
fn products() -> Vec<Product> {
    vec![
    Product {
        name: String::from("Antitranspirante pomelo 1/4 crema humectante Dove en aerosol 150 ml"),
        price_text: String::new(),
        price: 4564.91,
        ..Default::default()
    },
    Product {
        name: String::from("Desodorante Axe Gold vainilla en aerosol 150 ml"),
        price_text: String::new(),
        price: 3744.05,
        ..Default::default()
    },
    Product {
        name: String::from("Desodorante para hombre Axe Musk musk en aerosol 150 ml"),
        price_text: String::new(),
        price: 3744.05,
        ..Default::default()
    },
    Product {
        name: String::from("Desodorante para hombre Axe marine en aerosol 150 ml"),
        price_text: String::new(),
        price: 3744.05,
        ..Default::default()
    },
    Product {
        name: String::from("Repelente de insectos Livopen sports en aerosol 132 g"),
        price_text: String::new(),
        price: 4489.27,
        ..Default::default()
    },
]
}

#[test]
fn extracts_all_prices_from_compreahora_mobile_fixture() {
    common::assert_fixture(
        "tests/fixtures/compreahora_mobile.html",
        &products(),
        "styles-list-view-GbL",
    );
}

#[test]
fn extracts_all_prices_from_compreahora_desktop_fixture() {
    common::assert_fixture(
        "tests/fixtures/compreahora_desktop.html",
        &products(),
        "styles-list-view-GbL",
    );
}
