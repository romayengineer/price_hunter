use std::collections::{HashMap, HashSet};

use anyhow::Result;

use crate::store::types::{MatchInsert, ProductRow, ProviderProductRow};
use crate::store::Store;

/// Runs the fuzzy matcher between provider products and canonical
/// products. Every (provider product, canonical product) comparison is
/// scored and stored in `provider_product_matches` — pairs already
/// computed on a previous run are skipped, and each new score is written
/// immediately (one insert at a time) so a crash never loses progress.
/// After the cache is up to date, the best match per provider product is
/// linked (per-provider exclusivity) using the stored scores at or above
/// `MIN_SCORE`. Returns how many provider products were matched.
pub fn match_products(store: &Store) -> Result<usize> {
    let products = store.list_products()?;
    let provider_products = store.list_provider_products()?;

    let stored = store.list_all_matches(&provider_products)?;
    let mut stored_pairs: HashSet<(String, String)> = stored
        .iter()
        .map(|r| (r.provider_product_id.clone(), r.product_id.clone()))
        .collect();
    let mut candidates: Vec<crate::matching::MatchCandidate> = stored
        .iter()
        .filter(|r| r.score >= crate::matching::MIN_SCORE)
        .map(|r| crate::matching::MatchCandidate {
            provider_product_id: r.provider_product_id.clone(),
            product_id: r.product_id.clone(),
            score: r.score,
        })
        .collect();

    let inserted = backfill_comparisons(store, &provider_products, &products, &mut stored_pairs, &mut candidates)?;
    println!("Computed {inserted} new comparisons ({} already stored)", stored.len());

    finish_linking(store, &provider_products, &candidates)
}

/// Re-links provider products to canonical products using only the
/// comparisons already stored in `provider_product_matches` (no backfill).
/// A quick way to refresh links after scraping new data without waiting
/// for the full comparison pass — an interrupted `-match-products` can no
/// longer leave the matrix stale.
pub fn link_matches(store: &Store) -> Result<usize> {
    let provider_products = store.list_provider_products()?;
    let candidates = store.list_above_threshold_candidates()?;
    finish_linking(store, &provider_products, &candidates)
}

/// Clears `product_id` on every provider product and re-assigns winners
/// from `candidates`, printing how many provider products were matched.
fn finish_linking(
    store: &Store,
    provider_products: &[ProviderProductRow],
    candidates: &[crate::matching::MatchCandidate],
) -> Result<usize> {
    let provider_of: HashMap<&str, &str> = provider_products
        .iter()
        .map(|p| (p.id.as_str(), p.provider_id.as_str()))
        .collect();
    store.unlink_all(provider_products)?;
    let matched = link_winners(store, candidates, &provider_of)?;
    println!(
        "Matched {matched} of {} provider products",
        provider_products.len()
    );
    Ok(matched)
}

/// Scores and stores every (provider product, product) pair not already
/// present in `stored_pairs`, one insert at a time. New above-threshold
/// pairs are appended to `candidates`. Returns how many comparisons were
/// computed. A live progress percentage is redrawn on the same line.
fn backfill_comparisons(
    store: &Store,
    provider_products: &[ProviderProductRow],
    products: &[ProductRow],
    stored_pairs: &mut HashSet<(String, String)>,
    candidates: &mut Vec<crate::matching::MatchCandidate>,
) -> Result<usize> {
    let total = provider_products.len() * products.len();
    let mut inserted = 0;
    let mut done = 0usize;
    let mut last_pct = -1.0;
    for pp in provider_products {
        for product in products {
            inserted += backfill_pair(store, pp, product, stored_pairs, candidates)?;
            done += 1;
            print_progress(done, total, &mut last_pct);
        }
    }
    print_progress(total, total, &mut last_pct);
    println!();
    Ok(inserted)
}

/// Scores one pair unless it is already stored, writing the score
/// immediately. Returns 1 when a new comparison was computed, 0 when the
/// pair was already cached (including when another process just inserted
/// it).
fn backfill_pair(
    store: &Store,
    pp: &ProviderProductRow,
    product: &ProductRow,
    stored_pairs: &mut HashSet<(String, String)>,
    candidates: &mut Vec<crate::matching::MatchCandidate>,
) -> Result<usize> {
    let pair = (pp.id.clone(), product.id.clone());
    if stored_pairs.contains(&pair) {
        return Ok(0);
    }
    let score = crate::matching::similarity(&pp.name, &product.name);
    let created = matches!(
        store.create_match(&pair.0, &pair.1, score)?,
        MatchInsert::Created
    );
    stored_pairs.insert(pair);
    if score >= crate::matching::MIN_SCORE {
        candidates.push(crate::matching::MatchCandidate {
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
    store: &Store,
    candidates: &[crate::matching::MatchCandidate],
    provider_of: &HashMap<&str, &str>,
) -> Result<usize> {
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
    candidates: &[crate::matching::MatchCandidate],
    provider_of: &HashMap<&str, &str>,
) -> HashMap<String, Vec<crate::matching::MatchCandidate>> {
    let mut grouped: HashMap<String, Vec<crate::matching::MatchCandidate>> = HashMap::new();
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
fn apply_group(store: &Store, group: &[crate::matching::MatchCandidate]) -> Result<usize> {
    let mut matched = 0;
    for winner in crate::matching::assign_group(group) {
        store.link_product(&winner)?;
        matched += 1;
    }
    Ok(matched)
}

/// Redraws a `Progress: X.XX%` line in place (carriage return) without
/// spamming the terminal — only rewrites when the rounded percentage changes.
fn print_progress(done: usize, total: usize, last: &mut f64) {
    if total == 0 {
        return;
    }
    let pct = done as f64 * 100.0 / total as f64;
    if (pct - *last).abs() < 0.005 {
        return;
    }
    *last = pct;
    use std::io::Write;
    print!("\rProgress: {pct:.2}%");
    let _ = std::io::stdout().flush();
}