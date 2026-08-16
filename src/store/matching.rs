use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};

use super::Store;
use super::http::escape_filter;
use super::types::{
    PRODUCTS_COLLECTION, PROVIDER_PRODUCTS_COLLECTION, PROVIDER_PRODUCT_MATCHES_COLLECTION,
    MatchInsert, MatchListResponse, ProductLinkPayload, ProductRow, ProviderMatchPayload,
    ProviderMatchRow, ProviderProductRow,
};

impl Store {
    /// Runs the fuzzy matcher between provider products and canonical
    /// products. Every (provider product, canonical product) comparison is
    /// scored and stored in `provider_product_matches` — pairs already
    /// computed on a previous run are skipped, and each new score is written
    /// immediately (one insert at a time) so a crash never loses progress.
    /// After the cache is up to date, the best match per provider product is
    /// linked (per-provider exclusivity) using the stored scores at or above
    /// `MIN_SCORE`. Returns how many provider products were matched.
    pub fn match_products(&self) -> Result<usize> {
        let products = self.list_products()?;
        let provider_products = self.list_provider_products()?;
        let provider_of: HashMap<&str, &str> = provider_products
            .iter()
            .map(|p| (p.id.as_str(), p.provider_id.as_str()))
            .collect();

        let stored = self.list_all_matches(&provider_products)?;
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

        let inserted = self.backfill_comparisons(&provider_products, &products, &mut stored_pairs, &mut candidates)?;
        println!("Computed {inserted} new comparisons ({} already stored)", stored.len());

        self.unlink_all(&provider_products)?;
        let matched = self.link_winners(&candidates, &provider_of)?;
        println!(
            "Matched {matched} of {} provider products",
            provider_products.len()
        );
        Ok(matched)
    }

    /// Lists every canonical product with `active = true`.
    fn list_products(&self) -> Result<Vec<ProductRow>> {
        let mut items = Vec::new();
        let mut page = 1;
        loop {
            let result = self
                .client
                .records(PRODUCTS_COLLECTION)
                .list()
                .filter("active=true")
                .page(page)
                .per_page(100)
                .call::<ProductRow>()
                .context("could not list products")?;
            let count = result.items.len();
            items.extend(result.items);
            if count < 100 {
                break;
            }
            page += 1;
        }
        Ok(items)
    }

    /// Lists every provider product. Shared with the matrix builder.
    pub(super) fn list_provider_products(&self) -> Result<Vec<ProviderProductRow>> {
        let mut items = Vec::new();
        let mut page = 1;
        loop {
            let result = self
                .client
                .records(PROVIDER_PRODUCTS_COLLECTION)
                .list()
                .page(page)
                .per_page(100)
                .call::<ProviderProductRow>()
                .context("could not list provider products")?;
            let count = result.items.len();
            items.extend(result.items);
            if count < 100 {
                break;
            }
            page += 1;
        }
        Ok(items)
    }

    /// Lists every stored match. Loads each provider product's pairs with an
    /// indexed filter query (`provider_product_id=<id>`, ~1-2 ms) instead of
    /// paginating the whole table — a full OFFSET scan takes ~140 ms/page and
    /// scales with the cache size. Requests run in parallel workers, each with
    /// its own agent (ureq pools one connection per host by default, so a
    /// shared agent would serialize the workers).
    fn list_all_matches(&self, provider_products: &[ProviderProductRow]) -> Result<Vec<ProviderMatchRow>> {
        const WORKERS: usize = 16;
        let base = format!(
            "{}/api/collections/{}/records",
            self.client.base_url,
            PROVIDER_PRODUCT_MATCHES_COLLECTION
        );
        let token = self
            .client
            .auth_token
            .as_deref()
            .context("not authenticated to PocketBase")?
            .to_owned();
        let pp_ids: Vec<String> = provider_products.iter().map(|p| p.id.clone()).collect();
        let mut items = Vec::new();
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..WORKERS)
                .map(|w| {
                    let base = base.clone();
                    let token = token.clone();
                    let pp_ids = &pp_ids;
                    scope.spawn(move || {
                        let agent = ureq::Agent::new();
                        let mut out = Vec::new();
                        for i in (w..pp_ids.len()).step_by(WORKERS) {
                            out.extend(fetch_pp_matches(&agent, &base, &token, &pp_ids[i])?);
                        }
                        Ok::<Vec<ProviderMatchRow>, anyhow::Error>(out)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .map_err(|_| anyhow::anyhow!("match loader panicked"))?
                })
                .collect::<Vec<Result<Vec<ProviderMatchRow>>>>()
        });
        for result in results {
            items.extend(result?);
        }
        Ok(items)
    }

    /// Writes one `provider_product_matches` row for a computed comparison so
    /// the score survives a crash even if the rest of the run does not. A pair
    /// that already exists (unique index) is reported as `AlreadyExists`
    /// instead of failing, so a concurrent `-match-products` run can't abort
    /// this one with a 400.
    ///
    /// Uses the pooled agent (not the SDK's one-shot HTTP calls) so hundreds of
    /// thousands of sequential inserts reuse a single TCP connection instead
    /// of exhausting the OS ephemeral port range.
    fn create_match(
        &self,
        provider_product_id: &str,
        product_id: &str,
        score: f64,
    ) -> Result<MatchInsert> {
        let url = format!(
            "{}/api/collections/{}/records",
            self.client.base_url,
            PROVIDER_PRODUCT_MATCHES_COLLECTION
        );
        let token = self
            .client
            .auth_token
            .as_deref()
            .context("not authenticated to PocketBase")?;
        let body = serde_json::to_string(&ProviderMatchPayload {
            provider_product_id: provider_product_id.to_string(),
            product_id: product_id.to_string(),
            score,
            status: "pending".to_string(),
        })
        .context("could not serialize match")?;
        let response = self
            .agent
            .post(&url)
            .set("Authorization", token)
            .set("Content-Type", "application/json")
            .send_string(&body);
        match response {
            Ok(response) => {
                if !(200..300).contains(&response.status()) {
                    return Err(anyhow::anyhow!(
                        "could not write match: HTTP {} (pair {provider_product_id} x {product_id})",
                        response.status()
                    ));
                }
                Ok(MatchInsert::Created)
            }
            Err(ureq::Error::Status(status, response)) => {
                let detail = response.into_string().unwrap_or_default();
                if status == 400 && detail.contains("validation_not_unique") {
                    return Ok(MatchInsert::AlreadyExists);
                }
                Err(anyhow::anyhow!(
                    "could not write match: HTTP {status} body: {detail} (pair {provider_product_id} x {product_id})",
                ))
            }
            Err(e) => Err(anyhow::anyhow!("could not write match: {e}")),
        }
    }

    /// Nulls out `product_id` on every provider product so linking can be
    /// recomputed from the stored comparison cache. Match rows are kept.
    fn unlink_all(&self, provider_products: &[ProviderProductRow]) -> Result<()> {
        for pp in provider_products {
            self.client
                .records(PROVIDER_PRODUCTS_COLLECTION)
                .update(&pp.id, ProductLinkPayload { product_id: None })
                .call()
                .map_err(|e| anyhow::anyhow!("could not clear product link: {e}"))?;
        }
        Ok(())
    }

    /// Scores and stores every (provider product, product) pair not already
    /// present in `stored_pairs`, one insert at a time. New above-threshold
    /// pairs are appended to `candidates`. Returns how many comparisons were
    /// computed. A live progress percentage is redrawn on the same line.
    fn backfill_comparisons(
        &self,
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
                inserted +=
                    self.backfill_pair(pp, product, stored_pairs, candidates)?;
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
        &self,
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
            self.create_match(&pair.0, &pair.1, score)?,
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
        &self,
        candidates: &[crate::matching::MatchCandidate],
        provider_of: &HashMap<&str, &str>,
    ) -> Result<usize> {
        let grouped = self.group_by_provider(candidates, provider_of);
        let mut matched = 0;
        for group in grouped.values() {
            matched += self.apply_group(group)?;
        }
        Ok(matched)
    }

    /// Groups candidates by their provider id (owned values avoid borrow
    /// lifetime juggling).
    fn group_by_provider(
        &self,
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
    fn apply_group(&self, group: &[crate::matching::MatchCandidate]) -> Result<usize> {
        let mut matched = 0;
        for winner in crate::matching::assign_group(group) {
            self.link_product(&winner)?;
            matched += 1;
        }
        Ok(matched)
    }

    /// Links a winning provider product to its canonical product and marks the
    /// match row as confirmed.
    fn link_product(&self, winner: &crate::matching::MatchCandidate) -> Result<()> {
        self.client
            .records(PROVIDER_PRODUCTS_COLLECTION)
            .update(
                &winner.provider_product_id,
                ProductLinkPayload {
                    product_id: Some(winner.product_id.clone()),
                },
            )
            .call()
            .map_err(|e| anyhow::anyhow!("could not link product: {e}"))?;
        self.mark_confirmed(winner)?;
        Ok(())
    }

    /// Marks the match row for `(provider_product_id, product_id)` as
    /// confirmed.
    fn mark_confirmed(&self, winner: &crate::matching::MatchCandidate) -> Result<()> {
        let filter = format!(
            "provider_product_id='{}' && product_id='{}'",
            escape_filter(&winner.provider_product_id),
            escape_filter(&winner.product_id)
        );
        let existing = self
            .client
            .records(PROVIDER_PRODUCT_MATCHES_COLLECTION)
            .list()
            .filter(&filter)
            .per_page(1)
            .call::<ProviderMatchRow>()
            .context("could not look up match")?;
        if let Some(row) = existing.items.into_iter().next() {
            self.client
                .records(PROVIDER_PRODUCT_MATCHES_COLLECTION)
                .update(
                    &row.id,
                    ProviderMatchPayload {
                        provider_product_id: winner.provider_product_id.clone(),
                        product_id: winner.product_id.clone(),
                        score: winner.score,
                        status: "confirmed".to_string(),
                    },
                )
                .call()
                .map_err(|e| anyhow::anyhow!("could not confirm match: {e}"))?;
        }
        Ok(())
    }
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

/// Fetches all stored match rows for one provider product using the unique
/// index (a provider product maps to at most `products` rows, so a single
/// `per_page=500` request covers it).
fn fetch_pp_matches(
    agent: &ureq::Agent,
    base: &str,
    token: &str,
    provider_product_id: &str,
) -> Result<Vec<ProviderMatchRow>> {
    let filter = format!(
        "provider_product_id='{}'",
        escape_filter(provider_product_id)
    );
    let mut url = url::Url::parse(base).context("could not parse records url")?;
    url.query_pairs_mut()
        .append_pair("perPage", "500")
        .append_pair("filter", &filter);
    let res = agent
        .get(url.as_str())
        .set("Authorization", token)
        .call()
        .map_err(|e| anyhow::anyhow!("could not list matches: {e}"))?;
    let list: MatchListResponse = res
        .into_json()
        .map_err(|e| anyhow::anyhow!("could not parse matches: {e}"))?;
    Ok(list.items)
}
