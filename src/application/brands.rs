//! Use case: assign a brand to every provider product (`match_brands`).

use std::collections::HashMap;

use crate::domain::error::PriceStoreError;
use crate::domain::matching::{BRAND_MIN_SCORE, best_match, brand_coverage};
use crate::domain::model::ProviderProductRow;
use crate::domain::ports::PriceStore;

/// Result of one `-match-brands` run.
pub struct BrandMatchSummary {
    /// Provider products considered.
    pub provider_products: usize,
    /// Matched through a linked canonical product's brand.
    pub matched_from_product: usize,
    /// Matched by fuzzy brand coverage.
    pub matched_by_fuzzy: usize,
    /// Left without a brand.
    pub unmatched: usize,
    /// Provider products whose `brand_id` was written (changed).
    pub updated: usize,
}

/// Assigns a brand to every provider product and writes it to
/// `provider_products.brand_id` (only when it changed). A provider product
/// linked to a canonical product takes that product's brand; the rest are
/// fuzzy-matched against the brand table by token coverage. Unresolved
/// products keep `brand_id = null` so they can be found and their brands
/// added to the table.
pub fn match_brands(store: &impl PriceStore) -> Result<BrandMatchSummary, PriceStoreError> {
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
    Ok(BrandMatchSummary {
        provider_products: total,
        matched_from_product,
        matched_by_fuzzy,
        unmatched,
        updated,
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
) -> Result<(BrandSource, usize), PriceStoreError> {
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

/// A provider product whose stored name is missing the brand of its linked
/// canonical product. The provider site renders the brand, so the extractor
/// should have included it — every row here is a candidate extractor bug.
#[derive(Debug, PartialEq, Eq)]
pub struct MissingBrandRow {
    /// The provider product record id.
    pub provider_product_id: String,
    /// The provider record id.
    pub provider_id: String,
    /// The provider's hostname.
    pub provider_domain: String,
    /// The stored provider product name.
    pub name: String,
    /// The linked canonical product id.
    pub product_id: String,
    /// The canonical product's brand, absent from `name`.
    pub brand: String,
}

/// Outcome of one `-report-missing-brands` run.
#[derive(Debug, PartialEq, Eq)]
pub struct MissingBrandReport {
    /// Provider products with a `product_id` link.
    pub matched: usize,
    /// The subset whose name is missing the linked product's brand.
    pub affected: Vec<MissingBrandRow>,
}

/// Lists provider products linked to a canonical product whose stored name
/// does not contain that product's brand (`brand_coverage < 1.0`, so partial
/// brand tokens don't count as present). Names that already carry the brand
/// (all brand tokens, case-insensitive) are not reported.
pub fn missing_brands(store: &impl PriceStore) -> Result<MissingBrandReport, PriceStoreError> {
    let provider_products = store.list_provider_products()?;
    let products = store.list_all_products()?;
    let providers = store.list_providers()?;
    let brand_by_product: HashMap<&str, &str> = products
        .iter()
        .map(|p| (p.id.as_str(), p.brand.trim()))
        .collect();
    let domain_by_provider: HashMap<&str, &str> = providers
        .iter()
        .map(|p| (p.id.as_str(), p.domain.as_str()))
        .collect();
    let mut affected = Vec::new();
    let mut matched = 0;
    for pp in &provider_products {
        let Some(product_id) = pp.product_id.as_deref().filter(|s| !s.is_empty()) else {
            continue;
        };
        matched += 1;
        let Some(brand) = brand_by_product.get(product_id).copied() else {
            continue;
        };
        if brand.is_empty() || brand_coverage(&pp.name, brand) >= 1.0 {
            continue;
        }
        affected.push(MissingBrandRow {
            provider_product_id: pp.id.clone(),
            provider_id: pp.provider_id.clone(),
            provider_domain: domain_by_provider
                .get(pp.provider_id.as_str())
                .copied()
                .unwrap_or_default()
                .to_string(),
            name: pp.name.clone(),
            product_id: product_id.to_string(),
            brand: brand.to_string(),
        });
    }
    Ok(MissingBrandReport { matched, affected })
}

/// Returns every provider product whose name contains no known brand. A brand
/// is "in" the name when all its tokens appear (case-insensitive,
/// `brand_coverage == 1.0`); known brands are the `brand` table entries plus
/// the `brand` values of the canonical products. Rows returned here are
/// candidates for `-delete-unbranded`: after a re-scrape with brand-enriched
/// extraction, names without any brand are stale.
pub fn unbranded_products(
    store: &impl PriceStore,
) -> Result<Vec<ProviderProductRow>, PriceStoreError> {
    let provider_products = store.list_provider_products()?;
    let products = store.list_all_products()?;
    let brands = store.list_brands()?;
    let mut known_brands: Vec<String> = brands
        .iter()
        .map(|b| b.name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    known_brands.extend(
        products
            .iter()
            .map(|p| p.brand.trim())
            .filter(|brand| !brand.is_empty())
            .map(str::to_owned),
    );
    known_brands.sort();
    known_brands.dedup();
    if known_brands.is_empty() {
        return Ok(Vec::new());
    }
    Ok(provider_products
        .into_iter()
        .filter(|pp| {
            known_brands
                .iter()
                .all(|brand| brand_coverage(&pp.name, brand) < 1.0)
        })
        .collect())
}
