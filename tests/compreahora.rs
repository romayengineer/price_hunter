use price_hunter::detect::{detect_grid, Detection, Product};

// prices are under class mobile-product-price-price-number
fn products() -> Vec<Product> {
    vec![
    Product {
        name: String::from("Antitranspirante pomelo 1/4 crema humectante Dove en aerosol 150 ml"),
        price_text: String::new(),
        price: 4564.91,
    },
    Product {
        name: String::from("Desodorante Axe Gold vainilla en aerosol 150 ml"),
        price_text: String::new(),
        price: 3744.05
    },
    Product {
        name: String::from("Desodorante para hombre Axe Musk musk en aerosol 150 ml"),
        price_text: String::new(),
        price: 3744.05
    },
    Product {
        name: String::from("Desodorante para hombre Axe marine en aerosol 150 ml"),
        price_text: String::new(),
        price: 3744.05
    },
]
}

#[test]
fn extracts_all_prices_from_compreahora_fixture() {
    let html = std::fs::read_to_string("tests/fixtures/compreahora.html").expect("fixture missing");
    let detection = detect_grid(&html).expect("grid should be detected");
    list_products_are_found(&detection);
    assert_is_products_list(&detection);
}

fn list_products_are_found(detection: &Detection) {
    for expected in products() {
        let found = detection
            .products
            .iter()
            .any(|d| d.name == expected.name && d.price == expected.price);
        assert!(found, "expected product not found: {:?}", expected);
    }
}

fn assert_is_products_list(detection: &Detection) {
    assert!(
        detection
            .container
            .classes
            .iter()
            .any(|c| c == "styles-list-view-GbL"),
        "expected the products list as container, got {:?}",
        detection.container.classes
    );
}
