use price_hunter::detect::{detect_grid, Detection, Product};

const FABILU_URL: &str = "https://perfumeriasfabilu.com.ar/categoria/perfumeria/";

fn products() -> Vec<Product> {
    vec![
        Product {
            name: String::from("PAULVIC WOMAN X50ML"),
            price_text: String::new(),
            price: 8190.0,
        },
        Product {
            name: String::from("PAULVIC MEN X50ML"),
            price_text: String::new(),
            price: 8190.0,
        },
        Product {
            name: String::from("PIBES COLONIA X95ML"),
            price_text: String::new(),
            price: 7400.0,
        },
        Product {
            name: String::from("MUJERCITAS EDP X 40 ML"),
            price_text: String::new(),
            price: 7020.0,
        },
        Product {
            name: String::from("DANIELLE EDT X90ML"),
            price_text: String::new(),
            price: 7117.0,
        },
    ]
}

#[test]
fn extracts_all_prices_from_fabilu_fixture() {
    let html = std::fs::read_to_string("tests/fixtures/fabilu.html").expect("fixture missing");
    let detection = detect_grid(&html).expect("grid should be detected");
    list_products_are_found(&detection);
    assert_is_products_grid(&detection);
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

fn assert_is_products_grid(detection: &Detection) {
    assert!(
        detection.container.classes.iter().any(|c| c == "products"),
        "expected the products grid as container, got {:?}",
        detection.container.classes
    );
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
