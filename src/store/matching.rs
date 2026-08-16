use anyhow::{Context, Result};

use super::Store;
use super::http::escape_filter;
use super::types::{
    PROVIDER_PRODUCTS_COLLECTION, PROVIDER_PRODUCT_MATCHES_COLLECTION,
    MatchInsert, MatchListResponse, ProductLinkPayload, ProviderMatchPayload,
    ProviderMatchRow, ProviderProductRow,
};

impl Store {
    /// Lists every stored match. Loads each provider product's pairs with an
    /// indexed filter query (`provider_product_id=<id>`, ~1-2 ms) instead of
    /// paginating the whole table — a full OFFSET scan takes ~140 ms/page and
    /// scales with the cache size. Requests run in parallel workers, each with
    /// its own agent (ureq pools one connection per host by default, so a
    /// shared agent would serialize the workers).
    pub(crate) fn list_all_matches(
        &self,
        provider_products: &[ProviderProductRow],
    ) -> Result<Vec<ProviderMatchRow>> {
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
    pub(crate) fn create_match(
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
    pub(crate) fn unlink_all(&self, provider_products: &[ProviderProductRow]) -> Result<()> {
        for pp in provider_products {
            self.client
                .records(PROVIDER_PRODUCTS_COLLECTION)
                .update(&pp.id, ProductLinkPayload { product_id: None })
                .call()
                .map_err(|e| anyhow::anyhow!("could not clear product link: {e}"))?;
        }
        Ok(())
    }

    /// Links a winning provider product to its canonical product and marks the
    /// match row as confirmed.
    pub(crate) fn link_product(&self, winner: &crate::matching::MatchCandidate) -> Result<()> {
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
    pub(crate) fn mark_confirmed(&self, winner: &crate::matching::MatchCandidate) -> Result<()> {
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