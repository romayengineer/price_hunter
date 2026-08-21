use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

use crate::domain::detect::{Detection, Product};
use crate::domain::model::{ProviderProductRow, ProviderRow};
use crate::domain::time::{iso8601, now_secs};

use super::Store;
use super::error::Error;
use super::http::{escape_filter, host_of};
use super::types::{
    PROVIDER_PRODUCT_IMAGES_COLLECTION, PROVIDER_PRODUCT_PRICES_COLLECTION,
    PROVIDER_PRODUCTS_COLLECTION, PROVIDERS_COLLECTION, ProductImagePayload, ProductImageRow,
    ProviderPayload, ProviderPricePayload, ProviderProductPayload, SCRAPES_COLLECTION,
    ScrapePayload, ScrapeRow,
};

/// Generic page shape for a pooled list request.
#[derive(serde::Deserialize)]
struct Page<T> {
    items: Vec<T>,
}

impl Store {
    /// Persists one detection through the Record API:
    /// one `scrapes` record, then per detected product one `provider_products`
    /// (upserted by `(provider_id, name)` / `provider_product_url`), a
    /// `provider_product_prices` record only when the price changed, and its
    /// `provider_product_images` rows.
    ///
    /// All HTTP goes through the pooled agent so a capture with many products
    /// reuses one TCP connection instead of opening a fresh one per request
    /// (the SDK's one-shot client would exhaust macOS ephemeral ports).
    pub fn save(
        &self,
        url: &str,
        captured_at: u64,
        capture_path: &str,
        detection: &Detection,
    ) -> Result<(), Error> {
        self.save_inner(url, captured_at, capture_path, detection)?;
        Ok(())
    }

    /// Persists only `products` (assumed new — the caller has already seen the
    /// rest) under a scrape row that records `total_product_count`. Used by the
    /// auto-scrape loop to save each newly detected batch without re-processing
    /// products saved on an earlier step.
    pub fn save_incremental(
        &self,
        url: &str,
        captured_at: u64,
        capture_path: &str,
        total_product_count: usize,
        products: &[Product],
    ) -> Result<(), Error> {
        self.save_incremental_inner(
            url,
            captured_at,
            capture_path,
            total_product_count,
            products,
        )?;
        Ok(())
    }

    /// The `anyhow`-typed body behind [`Store::save`].
    fn save_inner(
        &self,
        url: &str,
        captured_at: u64,
        capture_path: &str,
        detection: &Detection,
    ) -> Result<()> {
        let host = host_of(url);
        let provider = self.ensure_provider(&host)?;
        let container_class = detection
            .container
            .classes
            .first()
            .cloned()
            .unwrap_or_default();
        let scrape = self.create_scrape(
            url,
            captured_at,
            capture_path,
            &provider.id,
            detection.products.len(),
            &container_class,
        )?;
        self.save_products(&provider, &scrape.id, &detection.products)?;
        Ok(())
    }

    /// The `anyhow`-typed body behind [`Store::save_incremental`].
    fn save_incremental_inner(
        &self,
        url: &str,
        captured_at: u64,
        capture_path: &str,
        total_product_count: usize,
        products: &[Product],
    ) -> Result<()> {
        let host = host_of(url);
        let provider = self.ensure_provider(&host)?;
        let scrape = self.create_scrape(
            url,
            captured_at,
            capture_path,
            &provider.id,
            total_product_count,
            "",
        )?;
        self.save_products(&provider, &scrape.id, products)?;
        Ok(())
    }

    /// Returns the existing provider for `domain` or creates it (name = domain,
    /// enabled = true).
    fn ensure_provider(&self, domain: &str) -> Result<ProviderRow> {
        let filter = format!("domain='{}'", escape_filter(domain));
        let url = self.records_url(PROVIDERS_COLLECTION, Some(&filter), 1)?;
        let page = self.agent_get_json::<Page<ProviderRow>>(&url)?;
        if let Some(row) = page.items.into_iter().next() {
            return Ok(row);
        }
        let created = self.agent_post_json::<ProviderRow>(
            &self.collection_url(PROVIDERS_COLLECTION),
            &serde_json::to_string(&ProviderPayload {
                domain: domain.to_string(),
                name: domain.to_string(),
                enabled: true,
            })?,
        )?;
        Ok(ProviderRow {
            id: created.id,
            domain: domain.to_string(),
            name: domain.to_string(),
            enabled: true,
            default_currency: None,
        })
    }

    fn create_scrape(
        &self,
        url: &str,
        captured_at: u64,
        capture_path: &str,
        provider_id: &str,
        product_count: usize,
        container_class: &str,
    ) -> Result<ScrapeRow> {
        let row = self.agent_post_json::<ScrapeRow>(
            &self.collection_url(SCRAPES_COLLECTION),
            &serde_json::to_string(&ScrapePayload {
                provider_id: provider_id.to_string(),
                url: url.to_string(),
                scraped_at: iso8601(captured_at),
                status: "success".to_string(),
                capture_path: capture_path.to_string(),
                product_count,
                container_class: container_class.to_string(),
            })?,
        )?;
        Ok(ScrapeRow { id: row.id })
    }

    /// Persists every product in `products` for `provider`: resolves each
    /// `provider_products` row (reusing existing ones, creating new ones),
    /// then writes its price and images. All HTTP is pooled.
    fn save_products(
        &self,
        provider: &ProviderRow,
        scrape_id: &str,
        products: &[Product],
    ) -> Result<()> {
        let existing = self.list_provider_products(&provider.id)?;
        let by_name: HashMap<&str, &ProviderProductRow> =
            existing.iter().map(|r| (r.name.as_str(), r)).collect();
        let mut new_ids: HashMap<String, String> = HashMap::new();

        for product in products {
            // A single bad product must not drop the rest of the capture: log
            // and move on so the other products still land.
            if let Err(e) = self.save_product(provider, scrape_id, product, &by_name, &mut new_ids)
            {
                log::error!("could not persist product {:?}: {e:#}", product.name);
            }
        }
        Ok(())
    }

    /// Resolves the `provider_products` id for one product, creating the row
    /// when neither `name` nor `provider_product_url` matches an existing one.
    /// Newly created ids are recorded in `new_ids` (keyed by name) so a
    /// duplicate name within one detection reuses the same row.
    fn save_product(
        &self,
        provider: &ProviderRow,
        scrape_id: &str,
        product: &Product,
        by_name: &HashMap<&str, &ProviderProductRow>,
        new_ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        let url = product.url.as_deref().unwrap_or("");
        let existing_id = by_name.get(product.name.as_str()).map(|r| r.id.clone());
        let is_new = existing_id.is_none() && !new_ids.contains_key(&product.name);
        let provider_product_id = match existing_id {
            Some(id) => id,
            None => {
                if let Some(id) = new_ids.get(&product.name) {
                    id.clone()
                } else {
                    let id = self.create_provider_product(provider, url, &product.name)?;
                    new_ids.insert(product.name.clone(), id.clone());
                    id
                }
            }
        };
        let currency = product
            .currency
            .clone()
            .or_else(|| provider.default_currency.clone())
            .unwrap_or_default();
        self.create_price(&provider_product_id, scrape_id, currency, product, is_new)?;
        self.sync_images(&provider_product_id, &product.images)?;
        Ok(())
    }

    /// Creates a `provider_products` row via the pooled agent and returns its id.
    fn create_provider_product(
        &self,
        provider: &ProviderRow,
        provider_product_url: &str,
        name: &str,
    ) -> Result<String> {
        let row = self.agent_post_json::<ProviderProductRow>(
            &self.collection_url(PROVIDER_PRODUCTS_COLLECTION),
            &serde_json::to_string(&ProviderProductPayload {
                provider_id: provider.id.clone(),
                provider_product_url: provider_product_url.to_string(),
                name: name.to_string(),
                last_seen_at: iso8601(now_secs()),
            })?,
        )?;
        Ok(row.id)
    }

    /// Inserts a price row only when it differs from the last recorded price
    /// for this provider product. The first observation is always recorded.
    /// A row is written when `price` or `currency` changed. For a brand-new
    /// provider product (`is_new`) the price is written directly — there can be
    /// no prior observation.
    fn create_price(
        &self,
        provider_product_id: &str,
        scrape_id: &str,
        currency: String,
        product: &Product,
        is_new: bool,
    ) -> Result<()> {
        let payload = ProviderPricePayload {
            provider_product_id: provider_product_id.to_string(),
            scrape_id: scrape_id.to_string(),
            price: product.price,
            currency: currency.clone(),
            price_text: product.price_text.clone(),
        };
        if is_new {
            self.agent_post_json::<serde_json::Value>(
                &self.collection_url(PROVIDER_PRODUCT_PRICES_COLLECTION),
                &serde_json::to_string(&payload)?,
            )?;
            return Ok(());
        }
        // Existing product: keep idempotency for this scrape and the
        // "only when changed" rule.
        let idempotency = format!(
            "provider_product_id='{}' && scrape_id='{}'",
            escape_filter(provider_product_id),
            escape_filter(scrape_id)
        );
        let idem_url =
            self.records_url(PROVIDER_PRODUCT_PRICES_COLLECTION, Some(&idempotency), 1)?;
        let existing = self.agent_get_json::<Page<super::types::PriceRow>>(&idem_url)?;
        if let Some(row) = existing.items.into_iter().next() {
            self.agent_patch_json::<serde_json::Value>(
                &self.record_url(PROVIDER_PRODUCT_PRICES_COLLECTION, &row.id),
                &serde_json::to_string(&payload)?,
            )?;
            return Ok(());
        }
        let last_url = self.records_url_sorted(
            PROVIDER_PRODUCT_PRICES_COLLECTION,
            &format!(
                "provider_product_id='{}'",
                escape_filter(provider_product_id)
            ),
            "-created",
            1,
        )?;
        let last = self.agent_get_json::<Page<super::types::PriceRow>>(&last_url)?;
        if matches!(
            last.items.into_iter().next(),
            Some(row) if row.price == product.price && row.currency == currency
        ) {
            return Ok(());
        }
        self.agent_post_json::<serde_json::Value>(
            &self.collection_url(PROVIDER_PRODUCT_PRICES_COLLECTION),
            &serde_json::to_string(&payload)?,
        )?;
        Ok(())
    }

    /// Upserts the product images keyed by url and removes rows that are no
    /// longer present. Position 0 is marked as the primary image. Pooled.
    fn sync_images(&self, provider_product_id: &str, images: &[String]) -> Result<()> {
        let filter = format!(
            "provider_product_id='{}'",
            escape_filter(provider_product_id)
        );
        let url = self.records_url(PROVIDER_PRODUCT_IMAGES_COLLECTION, Some(&filter), 100)?;
        let existing = self.agent_get_json::<Page<ProductImageRow>>(&url)?;

        for (position, url) in images.iter().enumerate() {
            self.upsert_image(provider_product_id, position, url, &existing.items)?;
        }
        self.remove_stale_images(&existing.items, images)?;
        Ok(())
    }

    fn upsert_image(
        &self,
        provider_product_id: &str,
        position: usize,
        url: &str,
        existing: &[ProductImageRow],
    ) -> Result<()> {
        let payload = ProductImagePayload {
            provider_product_id: provider_product_id.to_string(),
            url: url.to_string(),
            position,
            is_primary: position == 0,
        };
        match existing.iter().find(|row| row.url == url) {
            Some(row) => {
                self.agent_patch_json::<serde_json::Value>(
                    &self.record_url(PROVIDER_PRODUCT_IMAGES_COLLECTION, &row.id),
                    &serde_json::to_string(&payload)?,
                )?;
            }
            None => {
                self.agent_post_json::<serde_json::Value>(
                    &self.collection_url(PROVIDER_PRODUCT_IMAGES_COLLECTION),
                    &serde_json::to_string(&payload)?,
                )?;
            }
        }
        Ok(())
    }

    fn remove_stale_images(&self, existing: &[ProductImageRow], images: &[String]) -> Result<()> {
        for row in existing {
            if !images.contains(&row.url) {
                self.agent_delete(&self.record_url(PROVIDER_PRODUCT_IMAGES_COLLECTION, &row.id))?;
            }
        }
        Ok(())
    }

    /// Lists every `provider_products` row for `provider_id`, paginated, via the
    /// pooled agent.
    fn list_provider_products(&self, provider_id: &str) -> Result<Vec<ProviderProductRow>> {
        let filter = format!("provider_id='{}'", escape_filter(provider_id));
        let mut items = Vec::new();
        let mut page = 1;
        loop {
            let url =
                self.records_url_paged(PROVIDER_PRODUCTS_COLLECTION, Some(&filter), 100, page)?;
            let loaded = self.agent_get_json::<Page<ProviderProductRow>>(&url)?;
            let count = loaded.items.len();
            items.extend(loaded.items);
            if count < 100 {
                break;
            }
            page += 1;
        }
        Ok(items)
    }

    // ---- pooled HTTP helpers ----

    /// The base `.../api/collections/{collection}/records` URL.
    fn collection_url(&self, collection: &str) -> String {
        format!(
            "{}/api/collections/{collection}/records",
            self.client.base_url
        )
    }

    /// A `.../records/{id}` URL.
    fn record_url(&self, collection: &str, id: &str) -> String {
        format!("{}/{id}", self.collection_url(collection))
    }

    /// A records URL with an optional filter and page size (page 1).
    fn records_url(
        &self,
        collection: &str,
        filter: Option<&str>,
        per_page: usize,
    ) -> Result<String> {
        self.records_url_paged(collection, filter, per_page, 1)
    }

    /// A records URL with an optional filter, page size and page number.
    fn records_url_paged(
        &self,
        collection: &str,
        filter: Option<&str>,
        per_page: usize,
        page: usize,
    ) -> Result<String> {
        let mut url = url::Url::parse(&self.collection_url(collection))
            .context("could not parse records url")?;
        url.query_pairs_mut()
            .append_pair("perPage", &per_page.to_string());
        url.query_pairs_mut().append_pair("page", &page.to_string());
        if let Some(filter) = filter {
            url.query_pairs_mut().append_pair("filter", filter);
        }
        Ok(url.to_string())
    }

    /// A records URL with a `sort` parameter.
    fn records_url_sorted(
        &self,
        collection: &str,
        filter: &str,
        sort: &str,
        per_page: usize,
    ) -> Result<String> {
        let mut url = url::Url::parse(&self.collection_url(collection))
            .context("could not parse records url")?;
        url.query_pairs_mut()
            .append_pair("perPage", &per_page.to_string());
        url.query_pairs_mut().append_pair("page", "1");
        url.query_pairs_mut().append_pair("filter", filter);
        url.query_pairs_mut().append_pair("sort", sort);
        Ok(url.to_string())
    }

    /// The auth token for the pooled agent.
    fn auth_token(&self) -> Result<String> {
        self.client
            .auth_token
            .clone()
            .context("not authenticated to PocketBase")
    }

    /// Pooled GET that deserializes the JSON response.
    fn agent_get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let token = self.auth_token()?;
        let res = self
            .agent
            .get(url)
            .set("Authorization", &token)
            .call()
            .map_err(|e| anyhow::anyhow!("GET {url}: {e}"))?;
        res.into_json()
            .map_err(|e| anyhow::anyhow!("GET {url}: bad JSON: {e}"))
    }

    /// Pooled POST that deserializes the JSON response.
    fn agent_post_json<T: DeserializeOwned>(&self, url: &str, body: &str) -> Result<T> {
        let token = self.auth_token()?;
        let res = self
            .agent
            .post(url)
            .set("Authorization", &token)
            .set("Content-Type", "application/json")
            .send_string(body)
            .map_err(|e| anyhow::anyhow!("POST {url}: {e}"))?;
        res.into_json()
            .map_err(|e| anyhow::anyhow!("POST {url}: bad JSON: {e}"))
    }

    /// Pooled PATCH that deserializes the JSON response.
    fn agent_patch_json<T: DeserializeOwned>(&self, url: &str, body: &str) -> Result<T> {
        let token = self.auth_token()?;
        let res = self
            .agent
            .patch(url)
            .set("Authorization", &token)
            .set("Content-Type", "application/json")
            .send_string(body)
            .map_err(|e| anyhow::anyhow!("PATCH {url}: {e}"))?;
        res.into_json()
            .map_err(|e| anyhow::anyhow!("PATCH {url}: bad JSON: {e}"))
    }

    /// Pooled DELETE; a 404 counts as success.
    fn agent_delete(&self, url: &str) -> Result<()> {
        let token = self.auth_token()?;
        match self.agent.delete(url).set("Authorization", &token).call() {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(status, response)) => {
                if status == 404 {
                    return Ok(());
                }
                let detail = response.into_string().unwrap_or_default();
                Err(anyhow::anyhow!(
                    "DELETE {url}: HTTP {status} body: {detail}"
                ))
            }
            Err(e) => Err(anyhow::anyhow!("DELETE {url}: {e}")),
        }
    }
}
