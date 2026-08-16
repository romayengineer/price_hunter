use price_hunter::detect::{Detection, Product, detect_grid};

pub fn products_found(detection: &Detection, expected: &[Product]) {
    for product in expected {
        let found = detection
            .products
            .iter()
            .any(|d| d.name == product.name && d.price == product.price);
        assert!(found, "expected product not found: {product:?}");
    }
}

pub fn assert_container_class(detection: &Detection, expected_class: &str) {
    assert!(
        detection
            .container
            .classes
            .iter()
            .any(|c| c == expected_class),
        "expected the products container as {expected_class}, got {:?}",
        detection.container.classes
    );
}

pub fn assert_fixture(path: &str, expected: &[Product], container_class: &str) {
    let html = std::fs::read_to_string(path).expect("fixture missing");
    let detection = detect_grid(&html).expect("grid should be detected");
    products_found(&detection, expected);
    assert_container_class(&detection, container_class);
}
