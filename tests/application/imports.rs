//! Tests for the canonical-product promotion use case (`propose_unmatched`,
//! `create_product`) run against the in-memory [`FakeStore`](super::fakes::FakeStore).

use price_hunter::application::imports::propose_unmatched;
use price_hunter::domain::model::{BrandRow, ProductInsert, ProductRow, ProviderProductRow};
use price_hunter::domain::ports::ProductCatalog;

use super::fakes::FakeStore;

fn brand(id: &str, name: &str) -> BrandRow {
    BrandRow {
        id: id.to_string(),
        name: name.to_string(),
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

fn provider_product(id: &str, name: &str, product_id: Option<&str>) -> ProviderProductRow {
    ProviderProductRow {
        id: id.to_string(),
        provider_id: "prov1".to_string(),
        name: name.to_string(),
        product_id: product_id.map(str::to_owned),
        brand_id: None,
    }
}

#[test]
fn propose_unmatched_splits_brand_size_and_skips_linked_existing_and_duplicates() {
    let mut fake = FakeStore::default();
    fake.brands.push(brand("b1", "moschino"));
    fake.products
        .push(product("p1", "Diesel Fuel For Life EDT 125 ml"));

    fake.provider_products.push(provider_product(
        "pp1",
        "Moschino Gold Fresh Couture EDP 100 Ml",
        None,
    ));
    fake.provider_products
        .push(provider_product("pp2", "Whatever EDP 50 ml", Some("p9")));
    fake.provider_products.push(provider_product(
        "pp3",
        "Adidas Vibes Smooth Pace EDP Unisex 100 Ml",
        None,
    ));
    fake.provider_products.push(provider_product(
        "pp4",
        "MOSCHINO GOLD FRESH COUTURE EDP 100 ML",
        None,
    ));
    fake.provider_products
        .push(provider_product("pp5", "132 g", None));
    fake.provider_products.push(provider_product(
        "pp6",
        "Diesel Fuel For Life EDT 125 ml",
        None,
    ));

    let proposals = propose_unmatched(&fake).expect("proposals should succeed");

    assert_eq!(proposals.len(), 2);
    assert_eq!(
        proposals[0],
        price_hunter::application::imports::ProposedProduct {
            provider_product_id: "pp1".to_string(),
            source_name: "Moschino Gold Fresh Couture EDP 100 Ml".to_string(),
            brand: "moschino".to_string(),
            product_name: "Gold Fresh Couture EDP".to_string(),
            size: "100 ml".to_string(),
            name: "moschino Gold Fresh Couture EDP 100 ml".to_string(),
        }
    );
    assert_eq!(
        proposals[1],
        price_hunter::application::imports::ProposedProduct {
            provider_product_id: "pp3".to_string(),
            source_name: "Adidas Vibes Smooth Pace EDP Unisex 100 Ml".to_string(),
            brand: String::new(),
            product_name: "Adidas Vibes Smooth Pace EDP Unisex".to_string(),
            size: "100 ml".to_string(),
            name: "Adidas Vibes Smooth Pace EDP Unisex 100 ml".to_string(),
        }
    );
}

#[test]
fn create_product_records_and_reports_already_exists() {
    let fake = FakeStore::default();

    let first = fake
        .create_product(
            "moschino",
            "Gold Fresh Couture EDP",
            "moschino Gold Fresh Couture EDP 100 ml",
            "100 ml",
        )
        .expect("create should succeed");
    assert_eq!(first, ProductInsert::Created);
    let second = fake
        .create_product(
            "moschino",
            "Gold Fresh Couture EDP",
            "moschino Gold Fresh Couture EDP 100 ml",
            "100 ml",
        )
        .expect("create should succeed");
    assert_eq!(second, ProductInsert::AlreadyExists);
    assert_eq!(
        fake.created_products(),
        vec![(
            "moschino".to_string(),
            "Gold Fresh Couture EDP".to_string(),
            "100 ml".to_string(),
            "moschino Gold Fresh Couture EDP 100 ml".to_string(),
        )]
    );
}
