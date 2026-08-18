//! Domain entities shared across the application and infrastructure layers.
//! These are plain data types (no I/O); they get (de)serialized by the
//! PocketBase adapter and served over HTTP by the matrix server.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A canonical product used by the fuzzy matcher. `name` already holds the
/// full display name (brand + product_name + size); `brand` is the canonical
/// brand (also used to assign a brand to linked provider products).
#[derive(Default, Deserialize, Debug)]
pub struct ProductRow {
    /// The record id.
    pub id: String,
    /// The full display name (brand + product_name + size).
    pub name: String,
    /// The canonical brand name.
    pub brand: String,
    /// The raw product name without brand and size.
    #[serde(default)]
    pub product_name: String,
    /// The size (e.g. `100 ml`).
    #[serde(default)]
    pub size: String,
}

/// A provider product scraped from a store.
#[derive(Default, Deserialize, Debug)]
pub struct ProviderProductRow {
    /// The record id.
    pub id: String,
    /// The provider (`providers`) record id.
    pub provider_id: String,
    /// The scraped product name.
    pub name: String,
    /// The linked canonical product id, when set.
    pub product_id: Option<String>,
    /// The assigned brand id, when set.
    pub brand_id: Option<String>,
}

/// A provider (store) known to the system.
#[derive(Default, Deserialize, Debug)]
pub struct ProviderRow {
    /// The record id.
    pub id: String,
    /// The provider's hostname.
    pub domain: String,
    /// The provider's display name.
    pub name: String,
    /// Whether the provider is active.
    pub enabled: bool,
    /// The provider's default currency code, when known.
    pub default_currency: Option<String>,
}

/// A canonical brand.
#[derive(Default, Deserialize, Debug)]
pub struct BrandRow {
    /// The record id.
    pub id: String,
    /// The brand name.
    pub name: String,
}

/// One stored (provider product, canonical product) comparison.
#[derive(Default, Deserialize, Debug)]
pub struct ProviderMatchRow {
    /// The record id.
    pub id: String,
    /// The provider product id.
    pub provider_product_id: String,
    /// The canonical product id.
    pub product_id: String,
    /// The similarity score (0.0–1.0).
    pub score: f64,
    /// The match status (`pending`/`confirmed`).
    pub status: String,
}

/// Outcome of writing one comparison row.
#[derive(Debug, PartialEq, Eq)]
pub enum MatchInsert {
    /// The row was created.
    Created,
    /// The pair already exists (unique index) — e.g. inserted by a concurrent
    /// run — so it counts as already computed.
    AlreadyExists,
}

/// Outcome of inserting one canonical product.
#[derive(Debug, PartialEq, Eq)]
pub enum ProductInsert {
    /// The product was created.
    Created,
    /// A product with the same `(brand, product_name, size)` already exists
    /// (unique index), so the insert was skipped.
    AlreadyExists,
}

/// One provider column in the product × provider matrix.
#[derive(Serialize)]
pub struct MatrixProvider {
    /// The provider record id.
    pub id: String,
    /// The provider's hostname.
    pub domain: String,
    /// The provider's display name.
    pub name: String,
}

/// One product row in the matrix: the full display name (brand, product_name
/// and size joined) plus the latest price per provider id. Providers that
/// don't carry the product are simply absent from `prices`.
#[derive(Serialize)]
pub struct MatrixRow {
    /// The canonical product id.
    pub product_id: String,
    /// The full display name.
    pub name: String,
    /// The latest price per provider id.
    pub prices: HashMap<String, f64>,
}

/// The product × provider price matrix served by `GET /matrix`. Every row has
/// at least one linked provider product (no all-blank rows); columns include
/// every provider.
#[derive(Serialize)]
pub struct Matrix {
    /// When the matrix was built (ISO-8601, UTC).
    pub generated_at: String,
    /// One column per provider.
    pub providers: Vec<MatrixProvider>,
    /// One row per product priced at two or more providers.
    pub rows: Vec<MatrixRow>,
}
