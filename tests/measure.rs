#[test]
fn measure_containers() {
    let Some(path) = std::env::var("PRICE_HUNTER_MEASURE_FIXTURE").ok() else {
        return;
    };
    let html = std::fs::read_to_string(&path).expect("fixture missing");
    for candidate in price_hunter::detect::diagnose_containers(&html) {
        println!(
            "{} p={} d={} density={:.4} classes={:?} id={:?}",
            if candidate.selected { "SELECTED" } else { "        " },
            candidate.price_count,
            candidate.div_count,
            candidate.density,
            candidate.classes,
            candidate.id,
        );
    }
}
