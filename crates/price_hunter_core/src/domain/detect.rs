//! Pure HTML → [`Detection`] pipeline: finds the price grid in arbitrary
//! e-commerce HTML and extracts one [`Product`] per card. No browser needed.

#![allow(clippy::cognitive_complexity)]

use scraper::Html;
use serde::Serialize;

mod container;
mod extract;
mod prices;

#[cfg(test)]
mod tests;

pub use container::{ContainerCandidate, diagnose_containers};
use container::{best_container, build_container};
use extract::extract_products;
use prices::find_price_divs;

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
pub fn product_delta(
    products: &[Product],
    seen: &mut std::collections::HashSet<String>,
) -> Vec<Product> {
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

/// The result of running [`detect_grid`] over a page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Detection {
    /// The container that grouped the prices into a grid.
    pub container: Container,
    /// One product per detected card.
    pub products: Vec<Product>,
}

/// Detects the product price grid in `source` and extracts its products.
/// Returns `None` when no credible grid is found.
pub fn detect_grid(source: &str) -> Option<Detection> {
    detect_grid_with_brands(source, &[])
}

/// Detects the product price grid in `source` using a brand catalog.
/// `brands` is the set of known brand names (e.g. derived from the
/// canonical `products` table). When non-empty the extractor learns the DOM
/// path that holds brands across the grid (threshold ~25% of cards) and also
/// falls back to splitting the brand out of the name via token coverage.
/// Returns `None` when no credible grid is found.
pub fn detect_grid_with_brands(source: &str, brands: &[String]) -> Option<Detection> {
    let html = Html::parse_document(source);
    let price_divs = find_price_divs(&html);
    let (container_id, child_count) = best_container(&html, &price_divs)?;
    let container = build_container(&html, container_id, child_count);
    let products = if brands.is_empty() {
        extract_products(&html, container_id, &price_divs)
    } else {
        extract::extract_products_with_brands(&html, container_id, &price_divs, brands)
    };
    Some(Detection {
        container,
        products,
    })
}

/// Detects the grid using canonical products as brand source. Convenience
/// wrapper that extracts distinct `brand` values from `catalog` and delegates
/// to [`detect_grid_with_brands`].
pub fn detect_grid_with_products(
    source: &str,
    catalog: &[crate::domain::model::ProductRow],
) -> Option<Detection> {
    let mut brands: Vec<String> = catalog
        .iter()
        .map(|p| p.brand.trim())
        .filter(|b| !b.is_empty())
        .map(|b| b.to_string())
        .collect();
    brands.sort();
    brands.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    detect_grid_with_brands(source, &brands)
}
