use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::Store;
use super::http::escape_filter;
use super::types::{
    BRANDS_COLLECTION, BrandLinkPayload, MatchListResponse, PRODUCTS_COLLECTION,
    PROVIDER_PRODUCT_IMAGES_COLLECTION, PROVIDER_PRODUCT_MATCHES_COLLECTION,
    PROVIDER_PRODUCT_PRICES_COLLECTION, PROVIDER_PRODUCTS_COLLECTION, PROVIDERS_COLLECTION,
    ProductImportPayload, ProductImportRow, ProductLinkPayload, ProviderMatchPayload,
    ProviderPriceRow,
};
use crate::domain::error::PriceStoreError;
use crate::domain::matching::{MIN_SCORE, MatchCandidate};
use crate::domain::model::{
    BrandRow, MatchInsert, ProductInsert, ProductRow, ProviderMatchRow, ProviderProductRow,
    ProviderRow,
};
use crate::domain::ports::PriceStore;

impl Store {
    /// Lists every record of `collection` with an optional filter and sort,
    /// paginating `per_page` rows at a time until the collection is exhausted.
    /// Keeps the page/per_page loop in one place instead of duplicating it in
    /// every bulk-loading path.
    pub(crate) fn list_all<T>(
        &self,
        collection: &'static str,
        filter: Option<&str>,
        sort: Option<&str>,
        per_page: usize,
    ) -> Result<Vec<T>>
    where
        T: Default + DeserializeOwned,
    {
        let mut items = Vec::new();
        let mut page = 1;
        loop {
            let mut builder = self
                .client
                .records(collection)
                .list()
                .page(page)
                .per_page(per_page as i32);
            if let Some(filter) = filter {
                builder = builder.filter(filter);
            }
            if let Some(sort) = sort {
                builder = builder.sort(sort);
            }
            let result = builder
                .call::<T>()
                .with_context(|| format!("could not list {collection}"))?;
            let count = result.items.len();
            items.extend(result.items);
            if count < per_page {
                break;
            }
            page += 1;
        }
        Ok(items)
    }

    /// Deletes every row in `collection` that references `provider_product_id`
    /// (used to cascade-remove prices, images and match rows when a provider
    /// product is deleted).
    fn delete_related(&self, collection: &'static str, provider_product_id: &str) -> Result<()> {
        for id in self.list_related_ids(collection, provider_product_id)? {
            self.agent_destroy(collection, &id)?;
        }
        Ok(())
    }

    /// Lists the record ids in `collection` referencing `provider_product_id`,
    /// paginating `per_page=100` at a time. Goes through the pooled agent so
    /// a delete run stays on one TCP connection.
    fn list_related_ids(
        &self,
        collection: &'static str,
        provider_product_id: &str,
    ) -> Result<Vec<String>> {
        let base = format!(
            "{}/api/collections/{}/records",
            self.client.base_url, collection
        );
        let token = self
            .client
            .auth_token
            .as_deref()
            .context("not authenticated to PocketBase")?;
        let filter = format!(
            "provider_product_id='{}'",
            escape_filter(provider_product_id)
        );
        let mut ids = Vec::new();
        let mut page = 1;
        loop {
            let mut url = url::Url::parse(&base).context("could not parse records url")?;
            url.query_pairs_mut()
                .append_pair("perPage", "100")
                .append_pair("page", &page.to_string())
                .append_pair("filter", &filter);
            let res = self
                .agent
                .get(url.as_str())
                .set("Authorization", token)
                .call()
                .map_err(|e| anyhow::anyhow!("could not list {collection}: {e}"))?;
            let list: IdListResponse = res
                .into_json()
                .map_err(|e| anyhow::anyhow!("could not parse {collection} rows: {e}"))?;
            let count = list.items.len();
            ids.extend(list.items.into_iter().map(|row| row.id));
            if count < 100 {
                break;
            }
            page += 1;
        }
        Ok(ids)
    }

    /// Deletes one record through the pooled agent (not the SDK's one-shot
    /// HTTP calls) so the many requests of a delete run reuse a single TCP
    /// connection instead of exhausting the OS ephemeral port range. A 404
    /// counts as success (the row is already gone).
    fn agent_destroy(&self, collection: &'static str, id: &str) -> Result<()> {
        let url = format!(
            "{}/api/collections/{}/records/{id}",
            self.client.base_url, collection
        );
        let token = self
            .client
            .auth_token
            .as_deref()
            .context("not authenticated to PocketBase")?;
        match self.agent.delete(&url).set("Authorization", token).call() {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(status, response)) => {
                if status == 404 {
                    return Ok(());
                }
                let detail = response.into_string().unwrap_or_default();
                Err(anyhow::anyhow!(
                    "could not delete {collection} row {id}: HTTP {status} body: {detail}"
                ))
            }
            Err(e) => Err(anyhow::anyhow!(
                "could not delete {collection} row {id}: {e}"
            )),
        }
    }

    /// Marks the match row for `(provider_product_id, product_id)` as
    /// confirmed.
    pub(crate) fn mark_confirmed(&self, winner: &MatchCandidate) -> Result<()> {
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

impl PriceStore for Store {
    fn list_products(&self) -> Result<Vec<ProductRow>, PriceStoreError> {
        Ok(self.list_all(PRODUCTS_COLLECTION, Some("active=true"), None, 100)?)
    }

    fn list_all_products(&self) -> Result<Vec<ProductRow>, PriceStoreError> {
        Ok(self.list_all(PRODUCTS_COLLECTION, None, None, 100)?)
    }

    fn list_provider_products(&self) -> Result<Vec<ProviderProductRow>, PriceStoreError> {
        Ok(self.list_all(PROVIDER_PRODUCTS_COLLECTION, None, None, 100)?)
    }

    fn list_providers(&self) -> Result<Vec<ProviderRow>, PriceStoreError> {
        Ok(self.list_all(PROVIDERS_COLLECTION, None, None, 100)?)
    }

    fn list_brands(&self) -> Result<Vec<BrandRow>, PriceStoreError> {
        Ok(self.list_all(BRANDS_COLLECTION, None, None, 500)?)
    }

    fn list_above_threshold_candidates(&self) -> Result<Vec<MatchCandidate>, PriceStoreError> {
        let filter = format!("score>={MIN_SCORE}");
        let rows = self.list_all::<ProviderMatchRow>(
            PROVIDER_PRODUCT_MATCHES_COLLECTION,
            Some(&filter),
            None,
            500,
        )?;
        Ok(rows
            .into_iter()
            .map(|r| MatchCandidate {
                provider_product_id: r.provider_product_id,
                product_id: r.product_id,
                score: r.score,
            })
            .collect())
    }

    /// Lists every stored match. Loads each provider product's pairs with an
    /// indexed filter query (`provider_product_id=<id>`, ~1-2 ms) instead of
    /// paginating the whole table — a full OFFSET scan takes ~140 ms/page and
    /// scales with the cache size. Requests run in parallel workers, each with
    /// its own agent (ureq pools one connection per host by default, so a
    /// shared agent would serialize the workers).
    fn list_all_matches(
        &self,
        provider_products: &[ProviderProductRow],
    ) -> Result<Vec<ProviderMatchRow>, PriceStoreError> {
        const WORKERS: usize = 16;
        let base = format!(
            "{}/api/collections/{}/records",
            self.client.base_url, PROVIDER_PRODUCT_MATCHES_COLLECTION
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
    ) -> Result<MatchInsert, PriceStoreError> {
        let url = format!(
            "{}/api/collections/{}/records",
            self.client.base_url, PROVIDER_PRODUCT_MATCHES_COLLECTION
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
                    )
                    .into());
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
                )
                .into())
            }
            Err(e) => Err(anyhow::anyhow!("could not write match: {e}").into()),
        }
    }

    /// Nulls out `product_id` on every provider product so linking can be
    /// recomputed from the stored comparison cache. Match rows are kept.
    fn unlink_all(&self, provider_products: &[ProviderProductRow]) -> Result<(), PriceStoreError> {
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
    fn link_product(&self, winner: &MatchCandidate) -> Result<(), PriceStoreError> {
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

    fn update_brand_link(
        &self,
        provider_product_id: &str,
        brand_id: Option<&str>,
    ) -> Result<(), PriceStoreError> {
        self.client
            .records(PROVIDER_PRODUCTS_COLLECTION)
            .update(
                provider_product_id,
                BrandLinkPayload {
                    brand_id: brand_id.map(str::to_owned),
                },
            )
            .call()
            .map_err(|e| anyhow::anyhow!("could not update brand link: {e}"))?;
        Ok(())
    }

    /// Deletes a provider product after removing its match candidates,
    /// images and price history (PocketBase has no cascade deletes).
    fn delete_provider_product(&self, provider_product_id: &str) -> Result<(), PriceStoreError> {
        for collection in [
            PROVIDER_PRODUCT_MATCHES_COLLECTION,
            PROVIDER_PRODUCT_IMAGES_COLLECTION,
            PROVIDER_PRODUCT_PRICES_COLLECTION,
        ] {
            self.delete_related(collection, provider_product_id)?;
        }
        self.agent_destroy(PROVIDER_PRODUCTS_COLLECTION, provider_product_id)?;
        Ok(())
    }

    /// Inserts one canonical product (active) unless a product with the same
    /// `(brand, product_name, size)` already exists.
    fn create_product(
        &self,
        brand: &str,
        product_name: &str,
        name: &str,
        size: &str,
    ) -> Result<ProductInsert, PriceStoreError> {
        let filter = format!(
            "brand='{}' && product_name='{}' && size='{}'",
            escape_filter(brand),
            escape_filter(product_name),
            escape_filter(size)
        );
        let existing = self
            .client
            .records(PRODUCTS_COLLECTION)
            .list()
            .filter(&filter)
            .per_page(1)
            .call::<ProductImportRow>()
            .context("could not look up product")?;
        if existing.items.into_iter().next().is_some() {
            return Ok(ProductInsert::AlreadyExists);
        }
        self.client
            .records(PRODUCTS_COLLECTION)
            .create(ProductImportPayload {
                brand: brand.to_string(),
                product_name: product_name.to_string(),
                name: name.to_string(),
                size: size.to_string(),
                category: String::new(),
                active: true,
            })
            .call()
            .map_err(|e| anyhow::anyhow!("could not create product: {e}"))?;
        Ok(ProductInsert::Created)
    }

    fn latest_price_per_provider_product(&self) -> Result<HashMap<String, f64>, PriceStoreError> {
        let rows = self.list_all::<ProviderPriceRow>(
            PROVIDER_PRODUCT_PRICES_COLLECTION,
            None,
            Some("-created"),
            500,
        )?;
        let mut prices = HashMap::new();
        for row in rows {
            prices.entry(row.provider_product_id).or_insert(row.price);
        }
        Ok(prices)
    }
}

/// Page shape of a generic list response used to collect related row ids.
#[derive(Deserialize)]
struct IdListResponse {
    items: Vec<IdItem>,
}

#[derive(Deserialize)]
struct IdItem {
    id: String,
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
