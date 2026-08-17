use std::collections::{HashMap, HashSet};

use crate::application::reporter::Reporter;
use crate::domain::error::PriceStoreError;
use crate::domain::matching::{MIN_SCORE, MatchCandidate, assign_group, similarity};
use crate::domain::model::{MatchInsert, ProductRow, ProviderProductRow};
use crate::domain::ports::PriceStore;

/// Outcome of one `-match-products` run.
#[derive(Debug)]
pub struct MatchSummary {
    /// New comparisons scored and stored during the backfill.
    pub computed: usize,
    /// Comparisons already present from a previous run (skipped).
    pub already_stored: usize,
    /// Provider products considered for linking.
    pub provider_products: usize,
    /// Provider products linked to a canonical product.
    pub matched: usize,
}

/// Outcome of one `-link-matches` run (no backfill).
#[derive(Debug)]
pub struct LinkSummary {
    /// Provider products considered for linking.
    pub provider_products: usize,
    /// Provider products linked to a canonical product.
    pub matched: usize,
}

/// Runs the fuzzy matcher between provider products and canonical
/// products. Every (provider product, canonical product) comparison is
/// scored and stored in `provider_product_matches` — pairs already
/// computed on a previous run are skipped, and each new score is written
/// immediately (one insert at a time) so a crash never loses progress.
/// After the cache is up to date, the best match per provider product is
/// linked (per-provider exclusivity) using the stored scores at or above
/// `MIN_SCORE`. Progress is reported through `reporter`.
pub fn match_products(
    store: &impl PriceStore,
    reporter: &mut dyn Reporter,
) -> Result<MatchSummary, PriceStoreError> {
    let products = store.list_products()?;
    let provider_products = store.list_provider_products()?;

    let stored = store.list_all_matches(&provider_products)?;
    let mut stored_pairs: HashSet<(String, String)> = stored
        .iter()
        .map(|r| (r.provider_product_id.clone(), r.product_id.clone()))
        .collect();
    let mut candidates: Vec<MatchCandidate> = stored
        .iter()
        .filter(|r| r.score >= MIN_SCORE)
        .map(|r| MatchCandidate {
            provider_product_id: r.provider_product_id.clone(),
            product_id: r.product_id.clone(),
            score: r.score,
        })
        .collect();

    let computed = backfill_comparisons(
        store,
        &provider_products,
        &products,
        &mut stored_pairs,
        &mut candidates,
        reporter,
    )?;
    let (matched, total) = finish_linking(store, &provider_products, &candidates)?;

    Ok(MatchSummary {
        computed,
        already_stored: stored.len(),
        provider_products: total,
        matched,
    })
}

/// Re-links provider products to canonical products using only the
/// comparisons already stored in `provider_product_matches` (no backfill).
/// A quick way to refresh links after scraping new data without waiting
/// for the full comparison pass — an interrupted `-match-products` can no
/// longer leave the matrix stale.
pub fn link_matches(store: &impl PriceStore) -> Result<LinkSummary, PriceStoreError> {
    let provider_products = store.list_provider_products()?;
    let candidates = store.list_above_threshold_candidates()?;
    let (matched, total) = finish_linking(store, &provider_products, &candidates)?;
    Ok(LinkSummary {
        provider_products: total,
        matched,
    })
}

/// Clears `product_id` on every provider product and re-assigns winners
/// from `candidates`. Returns `(matched, provider_products)`.
fn finish_linking(
    store: &impl PriceStore,
    provider_products: &[ProviderProductRow],
    candidates: &[MatchCandidate],
) -> Result<(usize, usize), PriceStoreError> {
    let provider_of: HashMap<&str, &str> = provider_products
        .iter()
        .map(|p| (p.id.as_str(), p.provider_id.as_str()))
        .collect();
    store.unlink_all(provider_products)?;
    let matched = link_winners(store, candidates, &provider_of)?;
    Ok((matched, provider_products.len()))
}

/// Scores and stores every (provider product, product) pair not already
/// present in `stored_pairs`, one insert at a time. New above-threshold
/// pairs are appended to `candidates`. Returns how many comparisons were
/// computed.
fn backfill_comparisons(
    store: &impl PriceStore,
    provider_products: &[ProviderProductRow],
    products: &[ProductRow],
    stored_pairs: &mut HashSet<(String, String)>,
    candidates: &mut Vec<MatchCandidate>,
    reporter: &mut dyn Reporter,
) -> Result<usize, PriceStoreError> {
    let total = provider_products.len() * products.len();
    let mut computed = 0;
    let mut done = 0usize;
    for pp in provider_products {
        for product in products {
            computed += backfill_pair(store, pp, product, stored_pairs, candidates)?;
            done += 1;
            reporter.progress(done, total);
        }
    }
    reporter.progress(total, total);
    Ok(computed)
}

/// Scores one pair unless it is already stored, writing the score
/// immediately. Returns 1 when a new comparison was computed, 0 when the
/// pair was already cached (including when another process just inserted
/// it).
fn backfill_pair(
    store: &impl PriceStore,
    pp: &ProviderProductRow,
    product: &ProductRow,
    stored_pairs: &mut HashSet<(String, String)>,
    candidates: &mut Vec<MatchCandidate>,
) -> Result<usize, PriceStoreError> {
    let pair = (pp.id.clone(), product.id.clone());
    if stored_pairs.contains(&pair) {
        return Ok(0);
    }
    let score = similarity(&pp.name, &product.name);
    let created = matches!(
        store.create_match(&pair.0, &pair.1, score)?,
        MatchInsert::Created
    );
    stored_pairs.insert(pair);
    if score >= MIN_SCORE {
        candidates.push(MatchCandidate {
            provider_product_id: pp.id.clone(),
            product_id: product.id.clone(),
            score,
        });
    }
    Ok(usize::from(created))
}

/// Greedily assigns canonical products within each provider group, sets
/// `provider_products.product_id` and marks the winning match row
/// confirmed. Returns the number of provider products linked.
fn link_winners(
    store: &impl PriceStore,
    candidates: &[MatchCandidate],
    provider_of: &HashMap<&str, &str>,
) -> Result<usize, PriceStoreError> {
    let grouped = group_by_provider(candidates, provider_of);
    let mut matched = 0;
    for group in grouped.values() {
        matched += apply_group(store, group)?;
    }
    Ok(matched)
}

/// Groups candidates by their provider id (owned values avoid borrow
/// lifetime juggling).
fn group_by_provider(
    candidates: &[MatchCandidate],
    provider_of: &HashMap<&str, &str>,
) -> HashMap<String, Vec<MatchCandidate>> {
    let mut grouped: HashMap<String, Vec<MatchCandidate>> = HashMap::new();
    for c in candidates {
        if let Some(pid) = provider_of.get(c.provider_product_id.as_str()) {
            grouped
                .entry((*pid).to_string())
                .or_default()
                .push(c.clone());
        }
    }
    grouped
}

/// Assigns and links the winners of one provider group.
fn apply_group(
    store: &impl PriceStore,
    group: &[MatchCandidate],
) -> Result<usize, PriceStoreError> {
    let mut matched = 0;
    for winner in assign_group(group) {
        store.link_product(&winner)?;
        matched += 1;
    }
    Ok(matched)
}
