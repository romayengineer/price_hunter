//! Persistence ports for the application layer. Application use-cases depend on
//! these traits instead of any concrete store; the PocketBase adapter in
//! `infrastructure::store` is the implementation.
//!
//! Per ISP the former 13-method `PriceStore` god trait is split into five
//! focused traits (`ProductCatalog`, `BrandCatalog`, `ProviderCatalog`,
//! `MatchStore`, `PriceHistory`). The composite `PriceStore` remains as a
//! convenience supertrait so existing `&impl PriceStore` bounds keep working
//! via a blanket impl.

use std::collections::HashMap;

use crate::domain::error::PriceStoreError;
use crate::domain::matching::MatchCandidate;
use crate::domain::model::{
    BrandRow, MatchInsert, ProductInsert, ProductRow, ProviderMatchRow, ProviderProductRow,
    ProviderRow,
};

/// Catalog of canonical products.
pub trait ProductCatalog {
    /// Lists every canonical product with `active = true`.
    fn list_products(&self) -> Result<Vec<ProductRow>, PriceStoreError>;

    /// Lists every canonical product (no `active` filter — the matrix includes
    /// retired products that still have listings).
    fn list_all_products(&self) -> Result<Vec<ProductRow>, PriceStoreError>;

    /// Inserts one canonical product, reporting `AlreadyExists` when a product
    /// with the same `(brand, product_name, size)` is already present.
    fn create_product(
        &self,
        brand: &str,
        product_name: &str,
        name: &str,
        size: &str,
    ) -> Result<ProductInsert, PriceStoreError>;
}

/// Catalog of brands.
pub trait BrandCatalog {
    /// Lists every brand (id + name).
    fn list_brands(&self) -> Result<Vec<BrandRow>, PriceStoreError>;
}

/// Catalog of providers and their scraped products.
pub trait ProviderCatalog {
    /// Lists every provider.
    fn list_providers(&self) -> Result<Vec<ProviderRow>, PriceStoreError>;

    /// Lists every provider product.
    fn list_provider_products(&self) -> Result<Vec<ProviderProductRow>, PriceStoreError>;

    /// Writes `brand_id` on a provider product.
    fn update_brand_link(
        &self,
        provider_product_id: &str,
        brand_id: Option<&str>,
    ) -> Result<(), PriceStoreError>;

    /// Deletes a provider product and its related rows (prices, images and
    /// match candidates) so nothing orphaned is left behind.
    fn delete_provider_product(&self, provider_product_id: &str) -> Result<(), PriceStoreError>;
}

/// Store for fuzzy-match comparisons.
pub trait MatchStore {
    /// Loads every stored comparison at or above `MIN_SCORE` as linking
    /// candidates (filtered server-side, so it stays small even as the full
    /// comparison cache grows to millions of rows).
    fn list_above_threshold_candidates(&self) -> Result<Vec<MatchCandidate>, PriceStoreError>;

    /// Lists every stored match for the given provider products.
    fn list_all_matches(
        &self,
        provider_products: &[ProviderProductRow],
    ) -> Result<Vec<ProviderMatchRow>, PriceStoreError>;

    /// Writes one comparison row, reporting whether it was created or already
    /// existed (unique index).
    fn create_match(
        &self,
        provider_product_id: &str,
        product_id: &str,
        score: f64,
    ) -> Result<MatchInsert, PriceStoreError>;

    /// Nulls out `product_id` on every provider product so linking can be
    /// recomputed from the stored comparison cache.
    fn unlink_all(&self, provider_products: &[ProviderProductRow]) -> Result<(), PriceStoreError>;

    /// Links a winning provider product to its canonical product and marks the
    /// match row as confirmed.
    fn link_product(&self, winner: &MatchCandidate) -> Result<(), PriceStoreError>;
}

/// Latest price per provider product.
pub trait PriceHistory {
    /// Resolves the latest price per provider product (first row seen when
    /// listing prices newest-first).
    fn latest_price_per_provider_product(&self) -> Result<HashMap<String, f64>, PriceStoreError>;
}

/// Composite port — convenience supertrait for code that needs the whole
/// storage surface. Blanket-implemented for any `T` that implements all five
/// focused traits, so existing `&impl PriceStore` bounds remain valid.
pub trait PriceStore:
    ProductCatalog + BrandCatalog + ProviderCatalog + MatchStore + PriceHistory
{
}

impl<T> PriceStore for T where
    T: ProductCatalog + BrandCatalog + ProviderCatalog + MatchStore + PriceHistory
{
}
