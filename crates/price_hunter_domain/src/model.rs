//! Domain entities shared across the application and infrastructure layers.
//! These are plain data types (no I/O); they get (de)serialized by the
//! PocketBase adapter and served over HTTP by the matrix server.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// A canonical product used by the fuzzy matcher. `name` already holds the
/// full display name (brand + product_name); `brand` is the canonical
/// brand (also used to assign a brand to linked provider products).
#[derive(Default, Deserialize, Debug)]
pub struct ProductRow {
    /// The record id.
    pub id: String,
    /// The full display name (brand + product_name).
    pub name: String,
    /// The canonical brand name.
    pub brand: String,
    /// The raw product name without brand and size.
    #[serde(default)]
    pub product_name: String,
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
    /// The provider product URL slug, when captured.
    #[serde(default)]
    pub provider_product_url: String,
    /// The linked canonical product id, when set.
    pub product_id: Option<String>,
    /// The assigned brand id, when set.
    pub brand_id: Option<String>,
    /// The raw brand text extracted during scraping, when captured.
    /// Persisted as `provider_products.brand_name`; later resolved to
    /// `brand_id` by `match_brands` when the brand exists in the table.
    #[serde(default)]
    pub brand_name: Option<String>,
    /// The normalized size (e.g. `100 ml`), empty when unknown.
    #[serde(default)]
    pub size: String,
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
    /// A product with the same `(brand, product_name)` already exists
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
/// don't carry the product are simply absent from `prices`. Rows are per
/// `(product_id, size)` so the same product can appear multiple times for
/// different sizes.
#[derive(Serialize)]
pub struct MatrixRow {
    /// The canonical product id.
    pub product_id: String,
    /// The normalized size for this row (empty when unknown).
    pub size: String,
    /// The full display name (brand + product_name + size when present).
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

/// A price found inside a product card.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Price {
    /// The parsed numeric value.
    pub value: f64,
    /// The raw price text as it appeared in the markup.
    pub text: String,
}

/// A detected product: name, current price and the card's link/images.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Product {
    /// Display name, possibly enriched with the size and brand.
    pub name: String,
    /// The price text verbatim.
    pub price_text: String,
    /// The parsed price.
    pub price: f64,
    /// The product card's link, when one was found.
    #[serde(default)]
    pub url: Option<String>,
    /// Deduplicated image URLs in the card.
    #[serde(default)]
    pub images: Vec<String>,
    /// Best-effort currency code (`ARS`, `USD`, ...), when detectable.
    #[serde(default)]
    pub currency: Option<String>,
    /// Best-effort brand name extracted from the product card or from the
    /// name via catalog matching. `None` when no brand could be determined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
}

impl Product {
    /// Stable key that considers every field; two products with the same key
    /// are identical for delta purposes. `price` uses `to_bits` to avoid `f64`
    /// `Eq`/`Hash` issues and images are sorted so order is irrelevant.
    pub fn delta_key(&self) -> String {
        let mut imgs = self.images.clone();
        imgs.sort_unstable();
        format!(
            "{}|{}|{:016x}|{}|{}|{}|{}",
            self.name,
            self.price_text,
            self.price.to_bits(),
            self.url.as_deref().unwrap_or(""),
            imgs.join(","),
            self.currency.as_deref().unwrap_or(""),
            self.brand.as_deref().unwrap_or("")
        )
    }
}

/// Returns only the products not yet seen (inserting their `delta_key` into `seen`).
/// `seen` is in-memory only and tracks full product identity, not just name.
pub fn product_delta(products: &[Product], seen: &mut HashSet<String>) -> Vec<Product> {
    products
        .iter()
        .filter(|p| seen.insert(p.delta_key()))
        .cloned()
        .collect()
}

/// The grid container that was selected for the detection.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Container {
    /// The element's CSS classes.
    pub classes: Vec<String>,
    /// The element's `id` attribute, when present.
    pub id: Option<String>,
    /// Number of direct element children of the container.
    pub child_count: usize,
}

/// The result of running detection over a page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Detection {
    /// The container that grouped the prices into a grid.
    pub container: Container,
    /// One product per detected card.
    pub products: Vec<Product>,
}

/// One candidate product-grid container, as ranked by diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContainerCandidate {
    /// The candidate element's CSS classes.
    pub classes: Vec<String>,
    /// The candidate element's `id` attribute, when present.
    pub id: Option<String>,
    /// How many price divs the candidate contains.
    pub price_count: usize,
    /// How many nested divs the candidate contains.
    pub div_count: usize,
    /// `price_count / div_count`.
    pub density: f64,
    /// Whether detection would pick this candidate.
    pub selected: bool,
}
