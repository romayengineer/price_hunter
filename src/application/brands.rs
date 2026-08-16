use std::collections::HashMap;

use anyhow::Result;

use crate::domain::matching::{BRAND_MIN_SCORE, best_match, brand_coverage};
use crate::domain::model::ProviderProductRow;
use crate::domain::ports::PriceStore;

/// Result of one `-match-brands` run.
pub struct BrandMatchSummary {
    pub provider_products: usize,
    pub matched_from_product: usize,
    pub matched_by_fuzzy: usize,
    pub unmatched: usize,
}

/// Assigns a brand to every provider product and writes it to
/// `provider_products.brand_id` (only when it changed). A provider product
/// linked to a canonical product takes that product's brand; the rest are
/// fuzzy-matched against the brand table by token coverage. Unresolved
/// products keep `brand_id = null` so they can be found and their brands
/// added to the table.
pub fn match_brands(store: &impl PriceStore) -> Result<BrandMatchSummary> {
    let provider_products = store.list_provider_products()?;
    let products = store.list_products()?;
    let brands = store.list_brands()?;

    let brand_id_by_name: HashMap<&str, &str> = brands
        .iter()
        .map(|b| (b.name.as_str(), b.id.as_str()))
        .collect();
    let product_brand: HashMap<&str, Option<&str>> = products
        .iter()
        .map(|p| (p.id.as_str(), brand_id_by_name.get(p.brand.trim()).copied()))
        .collect();
    let brand_candidates: Vec<(String, String)> = brands
        .iter()
        .map(|b| (b.id.clone(), b.name.clone()))
        .collect();

    let mut matched_from_product = 0;
    let mut matched_by_fuzzy = 0;
    let mut unmatched = 0;
    let mut updated = 0usize;
    for pp in &provider_products {
        let (source, changed) = assign_brand(store, pp, &product_brand, &brand_candidates)?;
        updated += changed;
        match source {
            BrandSource::Product => matched_from_product += 1,
            BrandSource::Fuzzy => matched_by_fuzzy += 1,
            BrandSource::None => unmatched += 1,
        }
    }

    let total = provider_products.len();
    let matched = matched_from_product + matched_by_fuzzy;
    println!(
        "Brand-matched {matched} of {total} provider products \
         (product: {matched_from_product}, fuzzy: {matched_by_fuzzy}; {updated} updated)"
    );
    println!("Unmatched (brand_id null): {unmatched}");
    Ok(BrandMatchSummary {
        provider_products: total,
        matched_from_product,
        matched_by_fuzzy,
        unmatched,
    })
}

/// How a provider product got its brand.
enum BrandSource {
    Product,
    Fuzzy,
    None,
}

/// Resolves and writes the brand for one provider product, reporting the
/// match source and whether `brand_id` changed (1 = changed).
fn assign_brand(
    store: &impl PriceStore,
    pp: &ProviderProductRow,
    product_brand: &HashMap<&str, Option<&str>>,
    brand_candidates: &[(String, String)],
) -> Result<(BrandSource, usize)> {
    let target = resolve_brand(pp, product_brand, brand_candidates);
    // PocketBase serializes unset relations as `""`, so an empty value
    // counts as "not assigned" for change detection.
    let current = pp.brand_id.as_deref().filter(|s| !s.is_empty());
    let changed = usize::from(target != current);
    if changed == 1 {
        store.update_brand_link(&pp.id, target)?;
    }
    Ok((brand_source(pp, target), changed))
}

fn brand_source(pp: &ProviderProductRow, target: Option<&str>) -> BrandSource {
    match (
        target,
        pp.product_id.as_deref().is_some_and(|s| !s.is_empty()),
    ) {
        (Some(_), true) => BrandSource::Product,
        (Some(_), false) => BrandSource::Fuzzy,
        (None, _) => BrandSource::None,
    }
}

/// Resolves the brand for one provider product: the linked product's brand
/// when `product_id` is set (authoritative), otherwise the best fuzzy brand
/// match against the brand table. PocketBase reports unset relations as `""`,
/// so an empty `product_id` is treated as no link (falls through to fuzzy).
fn resolve_brand<'a>(
    pp: &ProviderProductRow,
    product_brand: &'a HashMap<&str, Option<&str>>,
    brand_candidates: &'a [(String, String)],
) -> Option<&'a str> {
    if let Some(product_id) = pp.product_id.as_deref()
        && !product_id.is_empty()
    {
        return product_brand.get(product_id).copied().flatten();
    }
    best_match(&pp.name, brand_candidates, brand_coverage, BRAND_MIN_SCORE).map(|(id, _, _)| id)
}
