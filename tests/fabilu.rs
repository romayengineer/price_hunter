use price_hunter::detect::{detect_grid, Detection, Product};

const FABILU_URL: &str = "https://perfumeriasfabilu.com.ar/categoria/perfumeria/";

const EXPECTED_PRICES: [f64; 12] = [
    5200.0, 6500.0, 7020.0, 7117.0, 7279.0, 7400.0, 7636.0, 8190.0, 8190.0, 13500.0, 21298.0,
    28500.0,
];

#[test]
fn extracts_all_prices_from_fabilu_fixture() {
    let html = std::fs::read_to_string("tests/fixtures/fabilu.html").expect("fixture missing");
    let detection = detect_grid(&html).expect("grid should be detected");
    assert_eq!(detection.products.len(), 12);
    prices_match_expected(&detection);
    every_product_is_named_and_priced(&detection);
    has_volume_digit_product(&detection);
    assert_is_products_grid(&detection);
}

fn prices_match_expected(detection: &Detection) {
    let mut prices: Vec<f64> = detection.products.iter().map(|p| p.price).collect();
    prices.sort_by(f64::total_cmp);
    let mut expected = EXPECTED_PRICES.to_vec();
    expected.sort_by(f64::total_cmp);
    assert_eq!(prices, expected);
}

fn every_product_is_named_and_priced(detection: &Detection) {
    for product in &detection.products {
        assert_product(product);
    }
}

fn assert_product(product: &Product) {
    assert!(!product.name.is_empty(), "product name empty");
    assert!(product.price > 0.0, "non-positive price");
}

fn has_volume_digit_product(detection: &Detection) {
    assert!(
        detection.products.iter().any(|p| p.name == "PAULVIC WOMAN X50ML"),
        "expected a named product with volume digits in its title"
    );
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

