use price_hunter::detect::{detect_grid, Detection, Product};

const EXPECTED_PRICES: [f64; 24] = [
    4564.91, 3744.05, 3744.05, 3744.05, 4489.27, 5836.05, 5053.99, 5053.99, 3175.52, 2942.54,
    2942.55, 4252.99, 2942.54, 4778.50, 2900.73, 2900.73, 2900.73, 2900.66, 4252.99, 2052.96,
    2052.96, 3744.05, 4564.64, 2897.72,
];

#[test]
fn extracts_all_prices_from_compreahora_fixture() {
    let html = std::fs::read_to_string("tests/fixtures/compreahora.html").expect("fixture missing");
    let detection = detect_grid(&html).expect("grid should be detected");
    assert_eq!(detection.products.len(), 24);
    prices_match_expected(&detection);
    every_product_is_named_and_priced(&detection);
    has_known_product(&detection);
    assert_is_products_list(&detection);
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

fn has_known_product(detection: &Detection) {
    assert!(
        detection.products.iter().any(|p| p.name
            == "Antitranspirante pomelo 1/4 crema humectante Dove en aerosol 150 ml"),
        "expected a named product with a fraction in its title"
    );
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
