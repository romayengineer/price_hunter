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
    pub id: String,
    pub name: String,
    pub brand: String,
}

/// A provider product scraped from a store.
#[derive(Default, Deserialize, Debug)]
pub struct ProviderProductRow {
    pub id: String,
    pub provider_id: String,
    pub name: String,
    pub product_id: Option<String>,
    pub brand_id: Option<String>,
}

/// A provider (store) known to the system.
#[derive(Default, Deserialize, Debug)]
pub struct ProviderRow {
    pub id: String,
    pub domain: String,
    pub name: String,
    pub enabled: bool,
    pub default_currency: Option<String>,
}

/// A canonical brand.
#[derive(Default, Deserialize, Debug)]
pub struct BrandRow {
    pub id: String,
    pub name: String,
}

/// One stored (provider product, canonical product) comparison.
#[derive(Default, Deserialize, Debug)]
pub struct ProviderMatchRow {
    pub id: String,
    pub provider_product_id: String,
    pub product_id: String,
    pub score: f64,
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

/// One provider column in the product × provider matrix.
#[derive(Serialize)]
pub struct MatrixProvider {
    pub id: String,
    pub domain: String,
    pub name: String,
}

/// One product row in the matrix: the full display name (brand, product_name
/// and size joined) plus the latest price per provider id. Providers that
/// don't carry the product are simply absent from `prices`.
#[derive(Serialize)]
pub struct MatrixRow {
    pub product_id: String,
    pub name: String,
    pub prices: HashMap<String, f64>,
}

/// The product × provider price matrix served by `GET /matrix`. Every row has
/// at least one linked provider product (no all-blank rows); columns include
/// every provider.
#[derive(Serialize)]
pub struct Matrix {
    pub generated_at: String,
    pub providers: Vec<MatrixProvider>,
    pub rows: Vec<MatrixRow>,
}
