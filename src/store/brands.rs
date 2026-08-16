use std::collections::HashMap;

use anyhow::{Context, Result};

use super::Store;
use super::types::{
    BRANDS_COLLECTION, PROVIDER_PRODUCTS_COLLECTION, BrandLinkPayload, BrandRow,
    ProviderProductRow,
};

/// Result of one `-match-brands` run.
pub struct BrandMatchSummary {
    pub provider_products: usize,
    pub matched_from_product: usize,
    pub matched_by_fuzzy: usize,
    pub unmatched: usize,
}

impl Store {
    /// Assigns a brand to every provider product and writes it to
    /// `provider_products.brand_id` (only when it changed). A provider product
    /// linked to a canonical product takes that product's brand; the rest are
    /// fuzzy-matched against the brand table by token coverage. Unresolved
    /// products keep `brand_id = null` so they can be found and their brands
    /// added to the table.
    pub fn match_brands(&self) -> Result<BrandMatchSummary> {
        let provider_products = self.list_provider_products()?;
        let products = self.list_products()?;
        let brands = self.list_brands()?;

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
            let target = resolve_brand(pp, &product_brand, &brand_candidates);
            if target != pp.brand_id.as_deref() {
                self.client
                    .records(PROVIDER_PRODUCTS_COLLECTION)
                    .update(&pp.id, BrandLinkPayload { brand_id: target.map(str::to_owned) })
                    .call()
                    .map_err(|e| anyhow::anyhow!("could not update brand link: {e}"))?;
                updated += 1;
            }
            if let Some(_) = target {
                if pp.product_id.is_some() {
                    matched_from_product += 1;
                } else {
                    matched_by_fuzzy += 1;
                }
            } else {
                unmatched += 1;
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

    /// Lists every brand (id + name).
    fn list_brands(&self) -> Result<Vec<BrandRow>> {
        let mut items = Vec::new();
        let mut page = 1;
        loop {
            let result = self
                .client
                .records(BRANDS_COLLECTION)
                .list()
                .page(page)
                .per_page(500)
                .call::<BrandRow>()
                .context("could not list brands")?;
            let count = result.items.len();
            items.extend(result.items);
            if count < 500 {
                break;
            }
            page += 1;
        }
        Ok(items)
    }
}

/// Resolves the brand for one provider product: the linked product's brand
/// when `product_id` is set (authoritative), otherwise the best fuzzy brand
/// match against the brand table.
fn resolve_brand<'a>(
    pp: &ProviderProductRow,
    product_brand: &'a HashMap<&str, Option<&str>>,
    brand_candidates: &'a [(String, String)],
) -> Option<&'a str> {
    if let Some(product_id) = &pp.product_id {
        return product_brand.get(product_id.as_str()).copied().flatten();
    }
    crate::matching::best_match(
        &pp.name,
        brand_candidates,
        crate::matching::brand_coverage,
        crate::matching::BRAND_MIN_SCORE,
    )
    .map(|(id, _, _)| id)
}
