//! Pure HTML → [`Detection`] pipeline: finds the price grid in arbitrary
//! e-commerce HTML and extracts one [`Product`] per card. No browser needed.
//!
//! Detection record types (`Product`, `Detection`, …) live in
//! `price_hunter_domain::model`; this module keeps only the scraper-based
//! traversal.

#![allow(clippy::cognitive_complexity)]

use scraper::Html;

mod container;
mod extract;
mod prices;

#[cfg(test)]
mod tests;

pub use container::diagnose_containers;
pub use price_hunter_domain::model::{
    Container, ContainerCandidate, Detection, Price, Product, product_delta,
};
use container::{best_container, build_container};
use extract::extract_products;

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
    catalog: &[price_hunter_domain::model::ProductRow],
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

use prices::find_price_divs;
