use anyhow::{Context, Result};
use pocketbase_sdk::client::{Auth, Client};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::detect::Detection;

const PRODUCTS_COLLECTION: &str = "products";
const PROVIDERS_COLLECTION: &str = "providers";
const SCRAPES_COLLECTION: &str = "scrapes";
const PROVIDER_PRODUCTS_COLLECTION: &str = "provider_products";
const PROVIDER_PRODUCT_IMAGES_COLLECTION: &str = "provider_product_images";
const PROVIDER_PRODUCT_MATCHES_COLLECTION: &str = "provider_product_matches";
const PROVIDER_PRODUCT_PRICES_COLLECTION: &str = "provider_product_prices";

/// Payload for the `products` collection.
#[derive(Serialize, Clone)]
struct ProductImportPayload {
    brand: String,
    product_name: String,
    name: String,
    size: String,
    category: String,
    active: bool,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
struct ProductImportRow {
    id: String,
}

/// Result of importing a single CSV row.
enum RowOutcome {
    Created,
    Skipped,
}

/// Payload for the `providers` collection.
#[derive(Serialize, Clone)]
struct ProviderPayload {
    domain: String,
    name: String,
    enabled: bool,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
struct ProviderRow {
    id: String,
    domain: String,
    name: String,
    enabled: bool,
    default_currency: Option<String>,
}

/// Payload for the `scrapes` collection.
#[derive(Serialize, Clone)]
struct ScrapePayload {
    provider_id: String,
    url: String,
    scraped_at: String,
    status: String,
    capture_path: String,
    product_count: usize,
    container_class: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct ScrapeRow {
    id: String,
}

/// Payload for the `provider_products` collection.
#[derive(Serialize, Clone)]
struct ProviderProductPayload {
    provider_id: String,
    provider_product_url: String,
    name: String,
    last_seen_at: String,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
struct ProviderProductRow {
    id: String,
    provider_id: String,
    name: String,
    product_id: Option<String>,
}

/// A canonical product used by the fuzzy matcher. `name` already holds the
/// full display name (brand + product_name + size).
#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
struct ProductRow {
    id: String,
    name: String,
}

/// Payload for updating `provider_products.product_id`. A `None` value
/// serializes as `null`, clearing the relation.
#[derive(Serialize, Clone)]
struct ProductLinkPayload {
    product_id: Option<String>,
}

/// Payload for the `provider_product_matches` collection.
#[derive(Serialize, Clone)]
struct ProviderMatchPayload {
    provider_product_id: String,
    product_id: String,
    score: f64,
    status: String,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
struct ProviderMatchRow {
    id: String,
}

/// Payload for the `provider_product_images` collection.
#[derive(Serialize, Clone)]
struct ProductImagePayload {
    provider_product_id: String,
    url: String,
    position: usize,
    is_primary: bool,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
struct ProductImageRow {
    id: String,
    url: String,
    position: usize,
}

#[derive(Default, Deserialize, Debug)]
#[allow(dead_code)]
struct PriceRow {
    id: String,
    price: f64,
    currency: String,
}

/// Payload for the `provider_product_prices` collection.
#[derive(Serialize, Clone)]
struct ProviderPricePayload {
    provider_product_id: String,
    scrape_id: String,
    price: f64,
    currency: String,
    price_text: String,
}

/// Persists detections to a running PocketBase through its Record API using the
/// normalized schema documented in DATABASE.md (providers, scrapes,
/// provider_products, provider_product_images, provider_product_prices).
///
/// The scraper NEVER writes SQL or touches the database file directly — all
/// writes go through the PocketBase HTTP API as an authenticated superuser
/// (superusers bypass collection rules, so no app user is required).
pub struct Store {
    client: Client<Auth>,
}

impl Store {
    /// Authenticates against a running PocketBase instance.
    ///
    /// Settings come from `~/.config/price_hunter/config.toml` (see
    /// `config::Config`) with `POCKETBASE_URL`, `POCKETBASE_SUPERUSER_EMAIL`
    /// and `POCKETBASE_SUPERUSER_PASSWORD` env vars overriding the file. The
    /// password is required (file or env).
    pub fn connect() -> Result<Self> {
        let config = crate::config::Config::load()?.with_env();
        let password = config.password().map(str::to_owned).with_context(|| {
            format!(
                "no PocketBase password configured — set the password in {} or export \
                 POCKETBASE_SUPERUSER_PASSWORD",
                crate::config::Config::path().display()
            )
        })?;
        let base_url = config.pocketbase.url;
        let email = config.pocketbase.email;
        let client = Client::new(&base_url)
            .superusers()
            .auth_with_password(&email, &password)
            .map_err(|e| {
                anyhow::anyhow!("could not authenticate to PocketBase at {base_url}: {e}")
            })?;
        Ok(Self { client })
    }

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
        product: &crate::detect::Product,
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

    /// Imports rows from a CSV with `brand,name,size` columns into the
    /// `products` collection. Rows already present (unique on
    /// `(brand, name, size)`) are skipped; the rest are created with
    /// `active = true`. Returns the number of rows created.
    pub fn import_products_csv(&self, path: &std::path::Path) -> Result<usize> {
        let mut reader = csv::Reader::from_path(path).with_context(|| {
            format!("could not read CSV at {}", path.display())
        })?;
        let mut created = 0usize;
        let mut skipped = 0usize;
        for result in reader.records() {
            let record = result.with_context(|| format!("could not parse CSV at {}", path.display()))?;
            match self.import_csv_row(&record)? {
                RowOutcome::Created => created += 1,
                RowOutcome::Skipped => skipped += 1,
            }
        }
        println!("Imported {created} products, skipped {skipped}");
        Ok(created)
    }

    /// Imports one CSV row into `products`, returning whether it was created
    /// or skipped as a duplicate. `product_name` keeps the raw CSV name while
    /// `name` holds the full display name (brand + product_name + size).
    fn import_csv_row(&self, record: &csv::StringRecord) -> Result<RowOutcome> {
        let brand = record.get(0).unwrap_or_default().trim().to_string();
        let product_name = record.get(1).unwrap_or_default().trim().to_string();
        let size = record.get(2).unwrap_or_default().trim().to_string();
        if product_name.is_empty() {
            return Ok(RowOutcome::Skipped);
        }
        if self
            .find_product(&brand, &product_name, &size)?
            .is_some()
        {
            return Ok(RowOutcome::Skipped);
        }
        let full_name = crate::matching::full_name(&brand, &product_name, &size);
        self.client
            .records(PRODUCTS_COLLECTION)
            .create(ProductImportPayload {
                brand,
                product_name,
                name: full_name,
                size,
                category: String::new(),
                active: true,
            })
            .call()
            .map_err(|e| anyhow::anyhow!("could not import product: {e}"))
            .map(|_| RowOutcome::Created)
    }

    /// Returns the existing canonical product for `(brand, product_name, size)`.
    fn find_product(
        &self,
        brand: &str,
        product_name: &str,
        size: &str,
    ) -> Result<Option<ProductImportRow>> {
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
        Ok(existing.items.into_iter().next())
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
        product: &crate::detect::Product,
    ) -> Result<()> {
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
            .create(ProviderPricePayload {
                provider_product_id: provider_product_id.to_string(),
                scrape_id: scrape_id.to_string(),
                price: product.price,
                currency,
                price_text: product.price_text.clone(),
            })
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

    /// Runs the fuzzy matcher between provider products and canonical
    /// products. Recomputes from scratch: existing matches and product links
    /// are cleared, all above-threshold comparisons are written to
    /// `provider_product_matches`, and the best match per provider product is
    /// linked (per-provider exclusivity). Returns how many provider products
    /// were matched.
    pub fn match_products(&self) -> Result<usize> {
        let products = self.list_products()?;
        let provider_products = self.list_provider_products()?;
        let provider_of: HashMap<&str, &str> = provider_products
            .iter()
            .map(|p| (p.id.as_str(), p.provider_id.as_str()))
            .collect();
        let candidates = crate::matching::above_threshold(
            &provider_products
                .iter()
                .map(|p| crate::matching::ProviderProduct {
                    id: p.id.clone(),
                    name: p.name.clone(),
                })
                .collect::<Vec<_>>(),
            &products
                .iter()
                .map(|p| crate::matching::Product {
                    id: p.id.clone(),
                    full_name: p.name.clone(),
                })
                .collect::<Vec<_>>(),
        );
        self.clear_previous_matches(&provider_products)?;
        self.write_candidates(&candidates)?;
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

    /// Lists every provider product.
    fn list_provider_products(&self) -> Result<Vec<ProviderProductRow>> {
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

    /// Clears every existing match row and nulls out `product_id` on all
    /// provider products so the matcher recomputes from scratch.
    fn clear_previous_matches(&self, provider_products: &[ProviderProductRow]) -> Result<()> {
        self.delete_all_matches()?;
        for pp in provider_products {
            self.client
                .records(PROVIDER_PRODUCTS_COLLECTION)
                .update(&pp.id, ProductLinkPayload { product_id: None })
                .call()
                .map_err(|e| anyhow::anyhow!("could not clear product link: {e}"))?;
        }
        Ok(())
    }

    /// Deletes every row in `provider_product_matches`.
    fn delete_all_matches(&self) -> Result<()> {
        let match_ids = self.list_match_ids()?;
        for id in match_ids {
            self.client
                .records(PROVIDER_PRODUCT_MATCHES_COLLECTION)
                .destroy(&id)
                .call()
                .map_err(|e| anyhow::anyhow!("could not delete match: {e}"))?;
        }
        Ok(())
    }

    /// Lists the ids of every row in `provider_product_matches`.
    fn list_match_ids(&self) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        let mut page = 1;
        loop {
            let result = self
                .client
                .records(PROVIDER_PRODUCT_MATCHES_COLLECTION)
                .list()
                .page(page)
                .per_page(100)
                .call::<ProviderMatchRow>()
                .context("could not list matches")?;
            let count = result.items.len();
            ids.extend(result.items.into_iter().map(|r| r.id));
            if count < 100 {
                break;
            }
            page += 1;
        }
        Ok(ids)
    }

    /// Writes every candidate pair to `provider_product_matches` with
    /// `status = "pending"`.
    fn write_candidates(&self, candidates: &[crate::matching::MatchCandidate]) -> Result<()> {
        for c in candidates {
            let payload = ProviderMatchPayload {
                provider_product_id: c.provider_product_id.clone(),
                product_id: c.product_id.clone(),
                score: c.score,
                status: "pending".to_string(),
            };
            self.client
                .records(PROVIDER_PRODUCT_MATCHES_COLLECTION)
                .create(payload)
                .call()
                .map_err(|e| anyhow::anyhow!("could not write match: {e}"))?;
        }
        Ok(())
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

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Formats a unix-seconds timestamp as the ISO-8601 string PocketBase expects
/// for `date` fields (`YYYY-MM-DD HH:MM:SS.mmmZ`, UTC). The store never sends
/// raw epoch numbers — PocketBase treats them as blank.
fn iso8601(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}.000Z")
}

/// Gregorian calendar day to (year, month, day) from the Unix epoch in days
/// (Howard Hinnant's `civil_from_days` algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Escapes a value for use inside a PocketBase filter string literal. Single
/// quotes and backslashes must be backslash-escaped or the filter parses
/// wrong (e.g. `name='A Drop d'Issey...'` → HTTP 400), which used to
/// abort the whole save and silently drop the rest of a capture.
fn escape_filter(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;
    use crate::detect::{Container, Product};

    #[test]
    fn host_of_extracts_host() {
        assert_eq!(
            host_of("https://www.parfumerie.com.ar/fragancias"),
            "www.parfumerie.com.ar"
        );
        assert_eq!(host_of("not a url"), "");
    }

    #[test]
    fn escape_filter_handles_apostrophes_and_backslashes() {
        assert_eq!(escape_filter("plain"), "plain");
        assert_eq!(escape_filter("A Drop d'Issey"), "A Drop d\\'Issey");
        assert_eq!(escape_filter(r"a\b"), r"a\\b");
        assert_eq!(escape_filter(r"back\'slash"), r"back\\\'slash");
    }

    #[test]
    fn iso8601_formats_utc_datetime() {
        assert_eq!(iso8601(0), "1970-01-01 00:00:00.000Z");
        assert_eq!(iso8601(1_234_567_890), "2009-02-13 23:31:30.000Z");
        assert_eq!(iso8601(123456), "1970-01-02 10:17:36.000Z");
    }

    #[test]
    fn scrape_payload_serializes_detection_fields() {
        let payload = ScrapePayload {
            provider_id: "prov-1".to_string(),
            url: "https://www.parfumerie.com.ar/fragancias".to_string(),
            scraped_at: iso8601(123456),
            status: "success".to_string(),
            capture_path: "captures/www.parfumerie.com.ar/capture-123456.json".to_string(),
            product_count: 2,
            container_class: "products".to_string(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["provider_id"], "prov-1");
        assert_eq!(json["url"], "https://www.parfumerie.com.ar/fragancias");
        assert_eq!(json["scraped_at"], "1970-01-02 10:17:36.000Z");
        assert_eq!(json["status"], "success");
        assert_eq!(
            json["capture_path"],
            "captures/www.parfumerie.com.ar/capture-123456.json"
        );
        assert_eq!(json["product_count"], 2);
        assert_eq!(json["container_class"], "products");
    }

    #[test]
    fn provider_product_payload_serializes_relation_and_key() {
        let payload = ProviderProductPayload {
            provider_id: "prov-1".to_string(),
            provider_product_url: "/a/light-blue-homme-edp-50".to_string(),
            name: String::new(),
            last_seen_at: iso8601(123456),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["provider_id"], "prov-1");
        assert_eq!(
            json["provider_product_url"],
            "/a/light-blue-homme-edp-50"
        );
        assert_eq!(json["name"], "");
        assert_eq!(json["last_seen_at"], "1970-01-02 10:17:36.000Z");
    }

    #[test]
    fn price_payload_serializes_relation_and_values() {
        let payload = ProviderPricePayload {
            provider_product_id: "pp-1".to_string(),
            scrape_id: "scr-1".to_string(),
            price: 242100.0,
            currency: "ARS".to_string(),
            price_text: "242.100".to_string(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["provider_product_id"], "pp-1");
        assert_eq!(json["scrape_id"], "scr-1");
        assert_eq!(json["price"], 242100.0);
        assert_eq!(json["currency"], "ARS");
        assert_eq!(json["price_text"], "242.100");
    }

    #[test]
    fn match_payload_serializes_relations_score_and_status() {
        let payload = ProviderMatchPayload {
            provider_product_id: "pp-1".to_string(),
            product_id: "prod-1".to_string(),
            score: 0.87,
            status: "confirmed".to_string(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["provider_product_id"], "pp-1");
        assert_eq!(json["product_id"], "prod-1");
        assert_eq!(json["score"], 0.87);
        assert_eq!(json["status"], "confirmed");
    }

    #[test]
    fn product_link_payload_serializes_some_as_id_and_none_as_null() {
        let linked = serde_json::to_value(ProductLinkPayload {
            product_id: Some("prod-1".to_string()),
        })
        .unwrap();
        assert_eq!(linked["product_id"], "prod-1");

        let cleared = serde_json::to_value(ProductLinkPayload { product_id: None }).unwrap();
        assert_eq!(cleared["product_id"], serde_json::Value::Null);
    }

    #[test]
    fn product_import_payload_serializes_csv_columns_and_active() {
        let payload = ProductImportPayload {
            brand: "adolfo dominguez".to_string(),
            product_name: "adn neroli ecstasy".to_string(),
            name: "adolfo dominguez adn neroli ecstasy 100 ml".to_string(),
            size: "100 ml".to_string(),
            category: String::new(),
            active: true,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["brand"], "adolfo dominguez");
        assert_eq!(json["product_name"], "adn neroli ecstasy");
        assert_eq!(json["name"], "adolfo dominguez adn neroli ecstasy 100 ml");
        assert_eq!(json["size"], "100 ml");
        assert_eq!(json["category"], "");
        assert_eq!(json["active"], true);
    }

    #[test]
    fn product_image_payload_marks_first_as_primary() {
        let payload = ProductImagePayload {
            provider_product_id: "pp-1".to_string(),
            url: "https://cdn.example/img/1.jpg".to_string(),
            position: 0,
            is_primary: true,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["provider_product_id"], "pp-1");
        assert_eq!(json["position"], 0);
        assert_eq!(json["is_primary"], true);
    }

    #[test]
    fn sample_detection_builds_one_capture_and_two_products() {
        let detection = Detection {
            container: Container {
                classes: vec!["products".to_string(), "row".to_string()],
                id: Some("grid-1".to_string()),
                child_count: 2,
            },
            products: vec![
                Product {
                    name: "Light Blue Homme EDP 50".to_string(),
                    price_text: "242.100".to_string(),
                    price: 242100.0,
                    ..Product::default()
                },
                Product {
                    name: "212 Vip EDP 80".to_string(),
                    price_text: "278.100".to_string(),
                    price: 278100.0,
                    ..Product::default()
                },
            ],
        };
        assert_eq!(detection.products.len(), 2);
        assert_eq!(detection.container.child_count, 2);
    }
}
