use price_hunter::detect::detect_grid;

const BEAUTY24_URL: &str = "https://www.beauty24.com.ar/perfumes-y-fragancias/";

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
