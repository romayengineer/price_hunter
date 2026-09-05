//! Use case: promote unmatched provider products to canonical `products`
//! rows (`import_unmatched`).

use std::collections::HashSet;

use crate::error::PriceStoreError;
use crate::matching::{
    BRAND_MIN_SCORE, best_match, brand_coverage, full_name, split_size, strip_brand,
};
use crate::model::ProviderProductRow;
use crate::ports::PriceStore;

/// A canonical product proposed from an unmatched provider product.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ProposedProduct {
    /// The provider product record id this came from.
    pub provider_product_id: String,
    /// The raw scraped name.
    pub source_name: String,
    /// The guessed brand (empty when none matched).
    pub brand: String,
    /// The product name without brand.
    pub product_name: String,
    /// The full display name (brand + product_name).
    pub name: String,
}

/// Builds one canonical product proposal per unmatched provider product
/// (`product_id` empty) whose name splits into a usable product name: brand is
/// guessed from the `brand` table (all brand tokens must appear), and the
/// remainder after stripping brand and trailing size becomes the product name.
/// Proposals that already exist in `products` (same full name) and duplicate
/// proposals are dropped.
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
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut proposals = Vec::new();
    for pp in &provider_products {
        if let Some(proposal) = propose_one(pp, &brand_candidates, &existing_names, &mut seen) {
            proposals.push(proposal);
        }
    }
    Ok(proposals)
}

/// Whether a (trimmed) brand cell is the `name` header row or blank, and so
/// should be skipped instead of imported as a brand.
pub fn brand_row_is_header_or_empty(name: &str) -> bool {
    name.is_empty() || name.eq_ignore_ascii_case("name")
}

/// Parses one `brand,product_name` CSV row (already trimmed by the caller
/// contract): `None` when `product_name` is empty and the row must be skipped.
pub fn parse_product_key(brand_cell: &str, product_name_cell: &str) -> Option<(String, String)> {
    let brand = brand_cell.trim().to_string();
    let product_name = product_name_cell.trim().to_string();
    if product_name.is_empty() {
        return None;
    }
    Some((brand, product_name))
}

/// Parses one `brand,product_name` CSV row into `(brand, product_name,
/// full_name)`: `None` when `product_name` is empty. `full_name` joins brand
/// and product name for the `products.name` display column.
pub fn parse_product_row(
    brand_cell: &str,
    product_name_cell: &str,
) -> Option<(String, String, String)> {
    let (brand, product_name) = parse_product_key(brand_cell, product_name_cell)?;
    let full = full_name(&brand, &product_name);
    Some((brand, product_name, full))
}

/// Parses one single-column brand CSV cell: `None` for the `name` header row,
/// blank rows and duplicates are handled by the caller.
pub fn parse_brand_row(name_cell: &str) -> Option<String> {
    let name = name_cell.trim();
    if brand_row_is_header_or_empty(name) {
        return None;
    }
    Some(name.to_string())
}

/// Builds one proposal from a single provider product, or `None` when it is
/// already linked, splits into an empty product name, already exists in
/// `products`, or duplicates an earlier proposal.
fn propose_one(
    pp: &ProviderProductRow,
    brand_candidates: &[(String, String)],
    existing_names: &HashSet<String>,
    seen: &mut HashSet<(String, String)>,
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
    let (without_size, _) = split_size(&without_brand);
    let product_name = without_size.trim();
    if product_name.is_empty() {
        return None;
    }
    let brand = brand.trim();
    let full = full_name(brand, product_name);
    let key = (
        brand.to_ascii_lowercase(),
        product_name.to_ascii_lowercase(),
    );
    if existing_names.contains(&full.to_ascii_lowercase()) || !seen.insert(key) {
        return None;
    }
    Some(ProposedProduct {
        provider_product_id: pp.id.clone(),
        source_name: name.to_string(),
        brand: brand.to_string(),
        product_name: product_name.to_string(),
        name: full,
    })
}

#[cfg(test)]
mod tests {
    use super::brand_row_is_header_or_empty;

    #[test]
    fn name_header_is_skipped_case_insensitively() {
        assert!(brand_row_is_header_or_empty("name"));
        assert!(brand_row_is_header_or_empty("NAME"));
        assert!(brand_row_is_header_or_empty("Name"));
    }

    #[test]
    fn empty_cell_is_skipped() {
        assert!(brand_row_is_header_or_empty(""));
    }

    #[test]
    fn real_brand_is_not_skipped() {
        assert!(!brand_row_is_header_or_empty("Natura"));
        assert!(!brand_row_is_header_or_empty("name brand"));
    }
}
