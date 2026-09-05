//! Tests for the matching use cases (`match_products`, `link_matches`) run
//! against the in-memory [`FakeStore`](super::fakes::FakeStore).

#![allow(clippy::cognitive_complexity)]

use price_hunter_domain::usecases::matching::{link_matches, match_products};
use price_hunter_domain::reporter::NoopReporter;
use price_hunter_domain::error::PriceStoreError;
use price_hunter_domain::model::{BrandRow, ProductRow, ProviderProductRow, ProviderRow};

use super::fakes::FakeStore;

fn provider(id: &str) -> ProviderRow {
    ProviderRow {
        id: id.to_string(),
        domain: "a.com.ar".to_string(),
        name: "A".to_string(),
        enabled: true,
        default_currency: None,
    }
}

fn product(id: &str, name: &str) -> ProductRow {
    ProductRow {
        id: id.to_string(),
        name: name.to_string(),
        brand: String::new(),
        ..Default::default()
    }
}

fn provider_product(id: &str, name: &str) -> ProviderProductRow {
    ProviderProductRow {
        id: id.to_string(),
        provider_id: "prov1".to_string(),
        name: name.to_string(),
        product_id: None,
        brand_id: None,
        ..Default::default()
    }
}

fn brand(id: &str, name: &str) -> BrandRow {
    BrandRow {
        id: id.to_string(),
        name: name.to_string(),
    }
}

fn branded_product(id: &str, brand: &str, name: &str) -> ProductRow {
    ProductRow {
        id: id.to_string(),
        name: name.to_string(),
        brand: brand.to_string(),
        ..Default::default()
    }
}

fn branded_provider_product(id: &str, name: &str, brand_id: &str) -> ProviderProductRow {
    ProviderProductRow {
        id: id.to_string(),
        provider_id: "prov1".to_string(),
        name: name.to_string(),
        product_id: None,
        brand_id: Some(brand_id.to_string()),
        ..Default::default()
    }
}

#[test]
fn match_products_scores_pairs_and_links_exact_winners() {
    let mut fake = FakeStore::default();
    fake.providers.push(provider("prov1"));
    fake.products
        .push(product("p1", "Diesel Fuel For Life EDT 125 ml"));
    fake.products
        .push(product("p2", "Adolfo Dominguez Adn Neroli Ecstasy 100 ml"));
    fake.provider_products
        .push(provider_product("pp1", "Diesel Fuel For Life EDT 125 ml"));
    fake.provider_products.push(provider_product(
        "pp2",
        "Adolfo Dominguez Adn Neroli Ecstasy 100 ml",
    ));

    let summary = match_products(&fake, &mut NoopReporter).expect("matching should succeed");

    assert_eq!(summary.computed, 2);
    assert_eq!(summary.already_stored, 0);
    assert_eq!(summary.provider_products, 2);
    assert_eq!(summary.matched, 2);
    let links = fake.product_links();
    assert_eq!(links.get("pp1"), Some(&Some("p1".to_string())));
    assert_eq!(links.get("pp2"), Some(&Some("p2".to_string())));
    // Only pairs scored at or above MIN_SCORE are stored: the two exact
    // matches (pp1×p1, pp2×p2) are kept, the two unrelated cross pairs are
    // scored but never written.
    assert_eq!(fake.matches().len(), 2);
    assert!(
        fake.matches().iter().all(|m| m.score >= 0.6),
        "only matches with score >= 0.6 are stored"
    );
}

#[test]
fn match_products_does_not_store_below_threshold_pairs() {
    let mut fake = FakeStore::default();
    fake.providers.push(provider("prov1"));
    fake.products
        .push(product("p1", "Diesel Fuel For Life EDT 125 ml"));
    fake.provider_products
        .push(provider_product("pp1", "Completely Unrelated Name"));

    let summary = match_products(&fake, &mut NoopReporter).expect("matching should succeed");

    assert_eq!(summary.computed, 0);
    assert_eq!(summary.matched, 0);
    assert_eq!(
        fake.matches().len(),
        0,
        "a below-threshold pair must not be stored"
    );
}

#[test]
fn match_products_links_only_the_best_candidate_per_product() {
    let mut fake = FakeStore::default();
    fake.providers.push(provider("prov1"));
    fake.products
        .push(product("p1", "Diesel Fuel For Life EDT 125 ml"));
    fake.provider_products
        .push(provider_product("pp1", "Diesel Fuel For Life EDT 125 ml"));
    fake.provider_products
        .push(provider_product("pp2", "Diesel Fuel For Life EDT 100 ml"));

    let summary = match_products(&fake, &mut NoopReporter).expect("matching should succeed");

    // pp1 scores higher (exact name) than pp2, so the shared product is only
    // claimed once.
    assert_eq!(summary.matched, 1);
    let links = fake.product_links();
    assert_eq!(links.get("pp1"), Some(&Some("p1".to_string())));
    assert_eq!(links.get("pp2"), Some(&None));
}

#[test]
fn match_products_skips_pairs_already_stored() {
    let mut fake = FakeStore::default();
    fake.providers.push(provider("prov1"));
    fake.products
        .push(product("p1", "Diesel Fuel For Life EDT 125 ml"));
    fake.provider_products
        .push(provider_product("pp1", "Diesel Fuel For Life EDT 125 ml"));
    fake.seed_match("pp1", "p1", 0.8);

    let summary = match_products(&fake, &mut NoopReporter).expect("matching should succeed");

    assert_eq!(summary.computed, 0);
    assert_eq!(summary.already_stored, 1);
    assert_eq!(summary.matched, 1);
    assert_eq!(
        fake.matches().len(),
        1,
        "an already-stored pair must not be re-inserted"
    );
    assert_eq!(fake.matches()[0].status, "confirmed");
    assert_eq!(
        fake.product_links().get("pp1"),
        Some(&Some("p1".to_string()))
    );
}

#[test]
fn link_matches_relinks_from_stored_comparisons() {
    let mut fake = FakeStore::default();
    fake.providers.push(provider("prov1"));
    fake.provider_products
        .push(provider_product("pp1", "Diesel Fuel For Life EDT 125 ml"));
    fake.seed_match("pp1", "p1", 0.9);

    let summary = link_matches(&fake).expect("linking should succeed");

    assert_eq!(summary.provider_products, 1);
    assert_eq!(summary.matched, 1);
    assert_eq!(
        fake.product_links().get("pp1"),
        Some(&Some("p1".to_string()))
    );
    assert_eq!(fake.matches()[0].status, "confirmed");
}

#[test]
fn match_products_partitions_by_brand_and_reports_pruning() {
    let mut fake = FakeStore::default();
    fake.providers.push(provider("prov1"));
    fake.brands.push(brand("b1", "Diesel"));
    fake.brands.push(brand("b2", "Adolfo Dominguez"));
    fake.products
        .push(branded_product("p1", "Diesel", "Diesel Fuel For Life EDT"));
    fake.products.push(branded_product(
        "p2",
        "Adolfo Dominguez",
        "Adolfo Dominguez ADN Neroli Ecstasy",
    ));
    fake.provider_products.push(branded_provider_product(
        "pp1",
        "Diesel Fuel For Life EDT 125 ml",
        "b1",
    ));
    fake.provider_products.push(branded_provider_product(
        "pp2",
        "Adolfo Dominguez ADN Neroli Ecstasy 100 ml",
        "b2",
    ));

    let summary = match_products(&fake, &mut NoopReporter).expect("matching should succeed");

    // Same-brand exact pairs link; the two cross-brand pairs are never even
    // scored (4 total, 2 evaluated, 2 skipped).
    assert_eq!(summary.computed, 2);
    assert_eq!(summary.matched, 2);
    assert_eq!(summary.evaluated, 2);
    assert_eq!(summary.skipped, 2);
    let links = fake.product_links();
    assert_eq!(links.get("pp1"), Some(&Some("p1".to_string())));
    assert_eq!(links.get("pp2"), Some(&Some("p2".to_string())));
    assert_eq!(fake.matches().len(), 2);
}

#[test]
fn match_products_unknown_brand_still_scans_every_group() {
    let mut fake = FakeStore::default();
    fake.providers.push(provider("prov1"));
    fake.brands.push(brand("b1", "Diesel"));
    fake.products
        .push(branded_product("p1", "Diesel", "Diesel Fuel For Life EDT"));
    fake.provider_products.push(provider_product(
        "pp1",
        "Diesel Fuel For Life EDT 125 ml",
    ));

    let summary = match_products(&fake, &mut NoopReporter).expect("matching should succeed");

    assert_eq!(summary.matched, 1);
    assert_eq!(
        fake.product_links().get("pp1"),
        Some(&Some("p1".to_string()))
    );
}

#[test]
fn match_products_propagates_typed_store_errors() {
    let mut fake = FakeStore::default();
    fake.fail = true;

    let err = match match_products(&fake, &mut NoopReporter) {
        Ok(_) => panic!("expected a store error"),
        Err(e) => e,
    };
    assert!(matches!(err, PriceStoreError::Request(_)));
}
