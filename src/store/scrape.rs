use anyhow::{Context, Result};

use crate::detect::{Detection, Product};

use super::Store;
use super::http::{escape_filter, host_of, iso8601, now_secs};
use super::types::{
    PROVIDER_PRODUCTS_COLLECTION, PROVIDER_PRODUCT_IMAGES_COLLECTION,
    PROVIDER_PRODUCT_PRICES_COLLECTION, PROVIDERS_COLLECTION, SCRAPES_COLLECTION,
    PriceRow, ProductImagePayload, ProductImageRow, ProviderProductPayload,
    ProviderProductRow, ProviderPricePayload, ProviderPayload, ProviderRow, ScrapePayload,
    ScrapeRow,
};

impl Store {
    /// Persists one detection through the Record API:
    /// one `scrapes` record, then per detected product one `provider_products`
    /// (upserted by `(provider_id, provider_product_url)`), a
    /// `provider_product_prices` record only when the price changed, and its
    /// `provider_product_images` rows.
    pub fn save(
        &self,
        url: &str,
        captured_at: u64,
        capture_path: &str,
        detection: &Detection,
    ) -> Result<()> {
        let host = host_of(url);
        let provider = self.ensure_provider(&host)?;
        let scrape = self.create_scrape(url, captured_at, capture_path, &provider.id, detection)?;
        for product in &detection.products {
            // A single bad product must not drop the rest of the capture: log
            // and move on so the other products still land (the scrape row is
            // already written).
            if let Err(e) = self.save_product(&provider, &scrape.id, product) {
                eprintln!("could not persist product {:?}: {e:#}", product.name);
            }
        }
        Ok(())
    }

    /// Persists one detected product: the `provider_products` row, its price
    /// (only when changed) and its images.
    fn save_product(
        &self,
        provider: &ProviderRow,
        scrape_id: &str,
        product: &Product,
    ) -> Result<()> {
        let provider_product = self.ensure_provider_product(
            provider,
            product.url.as_deref().unwrap_or(""),
            &product.name,
        )?;
        let currency = product
            .currency
            .clone()
            .or_else(|| provider.default_currency.clone())
            .unwrap_or_default();
        self.create_price(&provider_product.id, scrape_id, currency, product)?;
        self.sync_images(&provider_product.id, &product.images)?;
        Ok(())
    }

    /// Returns the existing provider for `domain` or creates it (name = domain,
    /// enabled = true).
    fn ensure_provider(&self, domain: &str) -> Result<ProviderRow> {
        let existing = self
            .client
            .records(PROVIDERS_COLLECTION)
            .list()
            .filter(&format!("domain='{}'", escape_filter(domain)))
            .per_page(1)
            .call::<ProviderRow>()
            .context("could not look up provider")?;
        if let Some(row) = existing.items.into_iter().next() {
            return Ok(row);
        }
        let created = self
            .client
            .records(PROVIDERS_COLLECTION)
            .create(ProviderPayload {
                domain: domain.to_string(),
                name: domain.to_string(),
                enabled: true,
            })
            .call()
            .map_err(|e| anyhow::anyhow!("could not create provider: {e}"))?;
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
        detection: &Detection,
    ) -> Result<ScrapeRow> {
        let container_class = detection.container.classes.first().cloned().unwrap_or_default();
        self.client
            .records(SCRAPES_COLLECTION)
            .create(ScrapePayload {
                provider_id: provider_id.to_string(),
                url: url.to_string(),
                scraped_at: iso8601(captured_at),
                status: "success".to_string(),
                capture_path: capture_path.to_string(),
                product_count: detection.products.len(),
                container_class,
            })
            .call()
            .map_err(|e| anyhow::anyhow!("could not create scrape: {e}"))
            .map(|r| ScrapeRow { id: r.id })
    }

    /// Returns the provider product for `(provider_id, name)`, falling
    /// back to the `(provider_id, provider_product_url)` match, creating it
    /// (with `name` and `last_seen_at` set) when neither exists.
    ///
    /// `name` is unique per provider, so a name that shows up under a
    /// new URL reuses the existing row instead of creating a duplicate.
    fn ensure_provider_product(
        &self,
        provider: &ProviderRow,
        provider_product_url: &str,
        name: &str,
    ) -> Result<ProviderProductRow> {
        if let Some(row) = self.find_provider_product(&provider.id, "name", name)? {
            return Ok(row);
        }
        if let Some(row) = self.find_provider_product(
            &provider.id,
            "provider_product_url",
            provider_product_url,
        )? {
            return Ok(row);
        }
        let created = self
            .client
            .records(PROVIDER_PRODUCTS_COLLECTION)
            .create(ProviderProductPayload {
                provider_id: provider.id.clone(),
                provider_product_url: provider_product_url.to_string(),
                name: name.to_string(),
                last_seen_at: iso8601(now_secs()),
            })
            .call()
            .map_err(|e| anyhow::anyhow!("could not create provider product: {e}"))?;
        Ok(ProviderProductRow {
            id: created.id,
            ..ProviderProductRow::default()
        })
    }

    fn find_provider_product(
        &self,
        provider_id: &str,
        field: &str,
        value: &str,
    ) -> Result<Option<ProviderProductRow>> {
        let filter = format!(
            "provider_id='{}' && {field}='{}'",
            escape_filter(provider_id),
            escape_filter(value)
        );
        let existing = self
            .client
            .records(PROVIDER_PRODUCTS_COLLECTION)
            .list()
            .filter(&filter)
            .per_page(1)
            .call::<ProviderProductRow>()
            .context("could not look up provider product")?;
        Ok(existing.items.into_iter().next())
    }

    /// Inserts a price row only when it differs from the last recorded price
    /// for this provider product. The first observation is always recorded.
    /// A row is written when `price` or `currency` changed.
    fn create_price(
        &self,
        provider_product_id: &str,
        scrape_id: &str,
        currency: String,
        product: &Product,
    ) -> Result<()> {
        let payload = ProviderPricePayload {
            provider_product_id: provider_product_id.to_string(),
            scrape_id: scrape_id.to_string(),
            price: product.price,
            currency: currency.clone(),
            price_text: product.price_text.clone(),
        };
        // Idempotency for this scrape: a capture can contain the same
        // provider product twice (same name/URL in one page). The unique
        // `(provider_product_id, scrape_id)` index makes a second insert
        // fail, so update the existing row instead.
        let existing = self
            .client
            .records(PROVIDER_PRODUCT_PRICES_COLLECTION)
            .list()
            .filter(&format!(
                "provider_product_id='{}' && scrape_id='{}'",
                escape_filter(provider_product_id),
                escape_filter(scrape_id)
            ))
            .per_page(1)
            .call::<PriceRow>()
            .context("could not look up existing price")?;
        if let Some(row) = existing.items.into_iter().next() {
            return self
                .client
                .records(PROVIDER_PRODUCT_PRICES_COLLECTION)
                .update(&row.id, payload)
                .call()
                .map_err(|e| anyhow::anyhow!("could not update price: {e}"))
                .map(|_| ());
        }
        let last = self
            .client
            .records(PROVIDER_PRODUCT_PRICES_COLLECTION)
            .list()
            .filter(&format!(
                "provider_product_id='{}'",
                escape_filter(provider_product_id)
            ))
            .sort("-created")
            .per_page(1)
            .call::<PriceRow>()
            .context("could not look up last price")?;
        if matches!(
            last.items.into_iter().next(),
            Some(row) if row.price == product.price && row.currency == currency
        ) {
            return Ok(());
        }
        self.client
            .records(PROVIDER_PRODUCT_PRICES_COLLECTION)
            .create(payload)
            .call()
            .map_err(|e| anyhow::anyhow!("could not create price: {e}"))
            .map(|_| ())
    }

    /// Upserts the product images keyed by url and removes rows that are no
    /// longer present. Position 0 is marked as the primary image.
    fn sync_images(&self, provider_product_id: &str, images: &[String]) -> Result<()> {
        let existing = self
            .client
            .records(PROVIDER_PRODUCT_IMAGES_COLLECTION)
            .list()
            .filter(&format!(
                "provider_product_id='{}'",
                escape_filter(provider_product_id)
            ))
            .per_page(100)
            .call::<ProductImageRow>()
            .context("could not look up product images")?;

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
            Some(row) => self
                .client
                .records(PROVIDER_PRODUCT_IMAGES_COLLECTION)
                .update(&row.id, payload)
                .call()
                .map(|_| ()),
            None => self
                .client
                .records(PROVIDER_PRODUCT_IMAGES_COLLECTION)
                .create(payload)
                .call()
                .map(|_| ()),
        }
        .map_err(|e| anyhow::anyhow!("could not write product image: {e}"))
    }

    fn remove_stale_images(&self, existing: &[ProductImageRow], images: &[String]) -> Result<()> {
        for row in existing {
            if !images.contains(&row.url) {
                self.client
                    .records(PROVIDER_PRODUCT_IMAGES_COLLECTION)
                    .destroy(&row.id)
                    .call()
                    .map_err(|e| anyhow::anyhow!("could not delete stale product image: {e}"))?;
            }
        }
        Ok(())
    }
}
