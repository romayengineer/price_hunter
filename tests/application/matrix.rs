//! Tests for the matrix use case (`matrix`) run against the in-memory
//! [`FakeStore`](super::fakes::FakeStore).

#![allow(clippy::cognitive_complexity)]

use std::collections::HashMap;

use price_hunter::domain::error::PriceStoreError;
use price_hunter::domain::model::{ProductRow, ProviderProductRow, ProviderRow};
use price_hunter::services::matrix::matrix;

use super::fakes::FakeStore;

fn provider(id: &str, domain: &str) -> ProviderRow {
    ProviderRow {
        id: id.to_string(),
        domain: domain.to_string(),
        name: domain.to_string(),
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

fn provider_product(id: &str, provider_id: &str, product_id: &str) -> ProviderProductRow {
    ProviderProductRow {
        id: id.to_string(),
        provider_id: provider_id.to_string(),
        name: String::new(),
        product_id: Some(product_id.to_string()),
        brand_id: None,
    }
}

#[test]
fn matrix_keeps_rows_with_two_providers_and_takes_the_lowest_price_per_provider() {
    let mut fake = FakeStore::default();
    fake.providers.push(provider("pA", "a.com.ar"));
    fake.providers.push(provider("pB", "b.com.ar"));
    fake.providers.push(provider("pC", "c.com.ar"));
    fake.products.push(product("prod1", "Alfa EDP 50 ml"));
    fake.products.push(product("prod2", "Beta EDP 100 ml"));
    // prod1 maps to two listings on pA and one on pB; prod2 only on pA.
    fake.provider_products
        .push(provider_product("pp1", "pA", "prod1"));
    fake.provider_products
        .push(provider_product("pp2", "pA", "prod1"));
    fake.provider_products
        .push(provider_product("pp3", "pB", "prod1"));
    fake.provider_products
        .push(provider_product("pp4", "pA", "prod2"));
    fake.seed_price("pp1", 100.0);
    fake.seed_price("pp2", 95.0);
    fake.seed_price("pp3", 90.0);
    fake.seed_price("pp4", 200.0);

    let matrix = matrix(&fake).expect("matrix should build");

    // prod2 is excluded: priced at only one provider.
    assert_eq!(matrix.rows.len(), 1);
    assert_eq!(matrix.rows[0].product_id, "prod1");
    assert_eq!(matrix.rows[0].name, "Alfa EDP 50 ml");
    assert_eq!(
        matrix.rows[0].prices,
        HashMap::from([("pA".to_string(), 95.0), ("pB".to_string(), 90.0)])
    );
    // Providers ordered by price count (tie broken by domain); the empty one
    // still appears, on the right.
    let domains: Vec<&str> = matrix.providers.iter().map(|p| p.domain.as_str()).collect();
    assert_eq!(domains, vec!["a.com.ar", "b.com.ar", "c.com.ar"]);
}

#[test]
fn matrix_propagates_typed_store_errors() {
    let mut fake = FakeStore::default();
    fake.fail = true;

    let err = match matrix(&fake) {
        Ok(_) => panic!("expected a store error"),
        Err(e) => e,
    };
    assert!(matches!(err, PriceStoreError::Request(_)));
}
