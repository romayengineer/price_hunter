//! Tests for the matching use cases (`match_products`, `link_matches`) run
//! against the in-memory [`FakeStore`](super::fakes::FakeStore).

use price_hunter::domain::error::PriceStoreError;
use price_hunter::domain::model::{ProductRow, ProviderProductRow, ProviderRow};
use price_hunter::services::matching::{link_matches, match_products};

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
    }
}

fn provider_product(id: &str, name: &str) -> ProviderProductRow {
    ProviderProductRow {
        id: id.to_string(),
        provider_id: "prov1".to_string(),
        name: name.to_string(),
        product_id: None,
        brand_id: None,
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

    let matched = match_products(&fake).expect("matching should succeed");

    assert_eq!(matched, 2);
    let links = fake.product_links();
    assert_eq!(links.get("pp1"), Some(&Some("p1".to_string())));
    assert_eq!(links.get("pp2"), Some(&Some("p2".to_string())));
    // Every (provider product × canonical product) pair is scored and stored.
    assert_eq!(fake.matches().len(), 4);
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

    let matched = match_products(&fake).expect("matching should succeed");

    // pp1 scores higher (exact name) than pp2, so the shared product is only
    // claimed once.
    assert_eq!(matched, 1);
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

    let matched = match_products(&fake).expect("matching should succeed");

    assert_eq!(matched, 1);
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

    let matched = link_matches(&fake).expect("linking should succeed");

    assert_eq!(matched, 1);
    assert_eq!(
        fake.product_links().get("pp1"),
        Some(&Some("p1".to_string()))
    );
    assert_eq!(fake.matches()[0].status, "confirmed");
}

#[test]
fn match_products_propagates_typed_store_errors() {
    let mut fake = FakeStore::default();
    fake.fail = true;

    let err = match match_products(&fake) {
        Ok(_) => panic!("expected a store error"),
        Err(e) => e,
    };
    assert!(matches!(err, PriceStoreError::Request(_)));
}
