//! Use case: promote unmatched provider products to canonical `products`
//! rows (`import_unmatched`).

use std::collections::HashSet;

use crate::domain::error::PriceStoreError;
use crate::domain::matching::{
    BRAND_MIN_SCORE, best_match, brand_coverage, full_name, split_size, strip_brand,
};
use crate::domain::model::ProviderProductRow;
use crate::domain::ports::PriceStore;

/// A canonical product proposed from an unmatched provider product.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ProposedProduct {
    /// The provider product record id this came from.
    pub provider_product_id: String,
    /// The raw scraped name.
    pub source_name: String,
    /// The guessed brand (empty when none matched).
    pub brand: String,
    /// The product name without brand and size.
    pub product_name: String,
    /// The size (e.g. `100 ml`), empty when none was detected.
    pub size: String,
    /// The full display name (brand + product_name + size).
    pub name: String,
}

/// Builds one canonical product proposal per unmatched provider product
/// (`product_id` empty) whose name splits into a usable product name: brand is
/// guessed from the `brand` table (all brand tokens must appear), size comes
/// from a trailing number (normalized to `N ml`), and the remainder becomes
/// the product name. Proposals that already exist in `products` (same full
/// name) and duplicate proposals are dropped.
pub fn propose_unmatched(store: &impl PriceStore) -> Result<Vec<ProposedProduct>, PriceStoreError> {
    let provider_products = store.list_provider_products()?;
    let products = store.list_all_products()?;
    let brands = store.list_brands()?;

    let brand_candidates: Vec<(String, String)> = brands
        .iter()
        .map(|b| (b.id.clone(), b.name.clone()))
        .collect();
    let existing_names: HashSet<String> = products
        .iter()
        .map(|p| {
            p.name
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase()
        })
        .collect();
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    let mut proposals = Vec::new();
    for pp in &provider_products {
        if let Some(proposal) = propose_one(pp, &brand_candidates, &existing_names, &mut seen) {
            proposals.push(proposal);
        }
    }
    Ok(proposals)
}

/// Builds one proposal from a single provider product, or `None` when it is
/// already linked, splits into an empty product name, already exists in
/// `products`, or duplicates an earlier proposal.
fn propose_one(
    pp: &ProviderProductRow,
    brand_candidates: &[(String, String)],
    existing_names: &HashSet<String>,
    seen: &mut HashSet<(String, String, String)>,
) -> Option<ProposedProduct> {
    let linked = pp.product_id.as_deref().is_some_and(|s| !s.is_empty());
    if linked {
        return None;
    }
    let name = pp.name.trim();
    if name.is_empty() {
        return None;
    }
    let brand = best_match(name, brand_candidates, brand_coverage, BRAND_MIN_SCORE)
        .map(|(_, text, _)| text)
        .unwrap_or_default();
    let without_brand = if brand.is_empty() {
        name.to_string()
    } else {
        strip_brand(name, brand)
    };
    let (without_size, size) = split_size(&without_brand);
    let product_name = without_size.trim();
    if product_name.is_empty() {
        return None;
    }
    let brand = brand.trim();
    let size = size.unwrap_or_default();
    let full = full_name(brand, product_name, &size);
    let key = (
        brand.to_ascii_lowercase(),
        product_name.to_ascii_lowercase(),
        size.to_ascii_lowercase(),
    );
    if existing_names.contains(&full.to_ascii_lowercase()) || !seen.insert(key) {
        return None;
    }
    Some(ProposedProduct {
        provider_product_id: pp.id.clone(),
        source_name: name.to_string(),
        brand: brand.to_string(),
        product_name: product_name.to_string(),
        size,
        name: full,
    })
}
