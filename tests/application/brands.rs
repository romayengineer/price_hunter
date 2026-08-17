//! Tests for the brand-assignment use case (`match_brands`) run against the
//! in-memory [`FakeStore`](super::fakes::FakeStore).

#![allow(clippy::cognitive_complexity)]

use price_hunter::domain::model::{BrandRow, ProductRow, ProviderProductRow};
use price_hunter::services::brands::match_brands;

use super::fakes::FakeStore;

#[test]
fn match_brands_assigns_product_brand_and_fuzzy_matches() {
    let mut fake = FakeStore::default();
    fake.brands.push(BrandRow {
        id: "b1".to_string(),
        name: "diesel".to_string(),
    });
    fake.products.push(ProductRow {
        id: "p1".to_string(),
        name: "Diesel Fuel For Life EDT 125 ml".to_string(),
        brand: "diesel".to_string(),
    });
    // pp1 is already linked to a canonical product -> takes its brand.
    fake.provider_products.push(ProviderProductRow {
        id: "pp1".to_string(),
        provider_id: "prov1".to_string(),
        name: "Diesel Fuel For Life EDT 125 ml".to_string(),
        product_id: Some("p1".to_string()),
        brand_id: None,
    });
    // pp2 is unlinked but its name carries the brand token -> fuzzy match.
    fake.provider_products.push(ProviderProductRow {
        id: "pp2".to_string(),
        provider_id: "prov1".to_string(),
        name: "Diesel Something".to_string(),
        product_id: None,
        brand_id: None,
    });
    // pp3 carries no brand token -> stays unresolved.
    fake.provider_products.push(ProviderProductRow {
        id: "pp3".to_string(),
        provider_id: "prov1".to_string(),
        name: "Unrelated Name".to_string(),
        product_id: None,
        brand_id: None,
    });

    let summary = match_brands(&fake).expect("brand matching should succeed");

    assert_eq!(summary.provider_products, 3);
    assert_eq!(summary.matched_from_product, 1);
    assert_eq!(summary.matched_by_fuzzy, 1);
    assert_eq!(summary.unmatched, 1);

    let links = fake.brand_links();
    assert_eq!(links.get("pp1"), Some(&Some("b1".to_string())));
    assert_eq!(links.get("pp2"), Some(&Some("b1".to_string())));
    assert!(
        !links.contains_key("pp3"),
        "unresolved products keep brand_id unset"
    );
}
