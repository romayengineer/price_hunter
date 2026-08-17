#[test]
fn probe_fixture() {
    let Some(path) = std::env::var("PRICE_HUNTER_PROBE_FIXTURE").ok() else {
        return;
    };
    let html = std::fs::read_to_string(&path).expect("fixture missing");
    let detection = price_hunter::detect::detect_grid(&html);
    println!("{:#?}", detection);
}
