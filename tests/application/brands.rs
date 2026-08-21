//! Tests for the brand-assignment use case (`match_brands`) run against the
//! in-memory [`FakeStore`](super::fakes::FakeStore).

#![allow(clippy::cognitive_complexity)]

use price_hunter::application::brands::{match_brands, missing_brands, unbranded_products};
use price_hunter::domain::model::{BrandRow, ProductRow, ProviderProductRow, ProviderRow};
use price_hunter::domain::ports::ProviderCatalog;

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
        ..Default::default()
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

#[test]
fn missing_brands_flags_matched_products_whose_name_lacks_the_brand() {
    let mut fake = FakeStore::default();
    fake.providers.push(ProviderRow {
        id: "prov1".to_string(),
        domain: "www.pigmento.com.ar".to_string(),
        ..Default::default()
    });
    fake.products.push(ProductRow {
        id: "p1".to_string(),
        name: "Moschino Gold Fresh Couture EDP 100 Ml".to_string(),
        brand: "moschino".to_string(),
        ..Default::default()
    });
    fake.products.push(ProductRow {
        id: "p2".to_string(),
        name: "Adidas Vibes Smooth Pace EDP Unisex 100 Ml".to_string(),
        brand: "adidas".to_string(),
        ..Default::default()
    });
    // pp1: matched but the name misses the brand -> flagged.
    fake.provider_products.push(ProviderProductRow {
        id: "pp1".to_string(),
        provider_id: "prov1".to_string(),
        name: "Gold Fresh Couture EDP 100 Ml".to_string(),
        product_id: Some("p1".to_string()),
        ..Default::default()
    });
    // pp2: matched and the name already carries the brand -> not flagged.
    fake.provider_products.push(ProviderProductRow {
        id: "pp2".to_string(),
        provider_id: "prov1".to_string(),
        name: "Adidas Vibes Smooth Pace EDP Unisex 100 Ml".to_string(),
        product_id: Some("p2".to_string()),
        ..Default::default()
    });
    // pp3: unlinked -> not counted as matched.
    fake.provider_products.push(ProviderProductRow {
        id: "pp3".to_string(),
        provider_id: "prov1".to_string(),
        name: "Something Else".to_string(),
        product_id: None,
        ..Default::default()
    });

    let report = missing_brands(&fake).expect("report should succeed");

    assert_eq!(report.matched, 2);
    assert_eq!(report.affected.len(), 1);
    let row = &report.affected[0];
    assert_eq!(row.provider_product_id, "pp1");
    assert_eq!(row.provider_domain, "www.pigmento.com.ar");
    assert_eq!(row.name, "Gold Fresh Couture EDP 100 Ml");
    assert_eq!(row.product_id, "p1");
    assert_eq!(row.brand, "moschino");
}

#[test]
fn unbranded_products_flags_names_without_any_known_brand() {
    let mut fake = FakeStore::default();
    fake.brands.push(BrandRow {
        id: "b1".to_string(),
        name: "moschino".to_string(),
    });
    fake.products.push(ProductRow {
        id: "p1".to_string(),
        name: "Adidas Vibes Smooth Pace EDP Unisex 100 Ml".to_string(),
        brand: "adidas".to_string(),
        ..Default::default()
    });
    // pp1 carries a brand token (from the brand table) -> not flagged.
    fake.provider_products.push(ProviderProductRow {
        id: "pp1".to_string(),
        provider_id: "prov1".to_string(),
        name: "Moschino Gold Fresh Couture EDP 100 Ml".to_string(),
        ..Default::default()
    });
    // pp2 carries a brand token (from products.brand) -> not flagged.
    fake.provider_products.push(ProviderProductRow {
        id: "pp2".to_string(),
        provider_id: "prov1".to_string(),
        name: "Adidas Vibes Smooth Pace EDP Unisex 100 Ml".to_string(),
        ..Default::default()
    });
    // pp3 carries no known brand -> flagged for deletion.
    fake.provider_products.push(ProviderProductRow {
        id: "pp3".to_string(),
        provider_id: "prov1".to_string(),
        name: "Gold Fresh Couture EDP 100 Ml".to_string(),
        ..Default::default()
    });

    let flagged = unbranded_products(&fake).expect("report should succeed");

    let ids: Vec<&str> = flagged.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["pp3"]);
}

#[test]
fn delete_provider_product_records_the_deletion() {
    let fake = FakeStore::default();
    fake.delete_provider_product("pp9")
        .expect("deletion should succeed");
    assert_eq!(fake.deleted_provider_products(), vec!["pp9".to_string()]);
}
