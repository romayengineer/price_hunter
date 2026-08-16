//! Persistence port for the application layer. Application use-cases depend on
//! this trait instead of any concrete store; the PocketBase adapter in
//! `infrastructure::store` is the implementation. Kept intentionally focused on
//! what the application actually needs — one real adapter, so no premature
//! split into finer-grained ports.

use std::collections::HashMap;

use anyhow::Result;

use crate::domain::matching::MatchCandidate;
use crate::domain::model::{
    BrandRow, MatchInsert, ProductRow, ProviderMatchRow, ProviderProductRow, ProviderRow,
};

/// The data-access surface the application layer orchestrates against.
pub trait PriceStore {
    /// Lists every canonical product with `active = true`.
    fn list_products(&self) -> Result<Vec<ProductRow>>;

    /// Lists every canonical product (no `active` filter — the matrix includes
    /// retired products that still have listings).
    fn list_all_products(&self) -> Result<Vec<ProductRow>>;

    /// Lists every provider product.
    fn list_provider_products(&self) -> Result<Vec<ProviderProductRow>>;

    /// Lists every provider.
    fn list_providers(&self) -> Result<Vec<ProviderRow>>;

    /// Lists every brand (id + name).
    fn list_brands(&self) -> Result<Vec<BrandRow>>;

    /// Loads every stored comparison at or above `MIN_SCORE` as linking
    /// candidates (filtered server-side, so it stays small even as the full
    /// comparison cache grows to millions of rows).
    fn list_above_threshold_candidates(&self) -> Result<Vec<MatchCandidate>>;

    /// Lists every stored match for the given provider products.
    fn list_all_matches(
        &self,
        provider_products: &[ProviderProductRow],
    ) -> Result<Vec<ProviderMatchRow>>;

    /// Writes one comparison row, reporting whether it was created or already
    /// existed (unique index).
    fn create_match(
        &self,
        provider_product_id: &str,
        product_id: &str,
        score: f64,
    ) -> Result<MatchInsert>;

    /// Nulls out `product_id` on every provider product so linking can be
    /// recomputed from the stored comparison cache.
    fn unlink_all(&self, provider_products: &[ProviderProductRow]) -> Result<()>;

    /// Links a winning provider product to its canonical product and marks the
    /// match row as confirmed.
    fn link_product(&self, winner: &MatchCandidate) -> Result<()>;

    /// Writes `brand_id` on a provider product.
    fn update_brand_link(&self, provider_product_id: &str, brand_id: Option<&str>) -> Result<()>;

    /// Resolves the latest price per provider product (first row seen when
    /// listing prices newest-first).
    fn latest_price_per_provider_product(&self) -> Result<HashMap<String, f64>>;
}
