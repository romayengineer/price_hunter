use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

use super::Store;
use super::types::{
    BRANDS_COLLECTION, PRODUCTS_COLLECTION, PROVIDERS_COLLECTION,
    PROVIDER_PRODUCTS_COLLECTION, PROVIDER_PRODUCT_MATCHES_COLLECTION,
    PROVIDER_PRODUCT_PRICES_COLLECTION, BrandLinkPayload, BrandRow, ProductRow,
    ProviderPriceRow, ProviderProductRow, ProviderRow,
};

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

    /// Lists every canonical product with `active = true`. Shared by the
    /// product and brand matchers.
    pub(crate) fn list_products(&self) -> Result<Vec<ProductRow>> {
        self.list_all(PRODUCTS_COLLECTION, Some("active=true"), None, 100)
    }

    /// Lists every canonical product (no `active` filter — the matrix includes
    /// retired products that still have listings).
    pub(crate) fn list_all_products(&self) -> Result<Vec<ProductRow>> {
        self.list_all(PRODUCTS_COLLECTION, None, None, 100)
    }

    /// Lists every provider product. Shared by the matchers and the matrix
    /// builder.
    pub(crate) fn list_provider_products(&self) -> Result<Vec<ProviderProductRow>> {
        self.list_all(PROVIDER_PRODUCTS_COLLECTION, None, None, 100)
    }

    /// Lists every provider.
    pub(crate) fn list_providers(&self) -> Result<Vec<ProviderRow>> {
        self.list_all(PROVIDERS_COLLECTION, None, None, 100)
    }

    /// Lists every brand (id + name).
    pub(crate) fn list_brands(&self) -> Result<Vec<BrandRow>> {
        self.list_all(BRANDS_COLLECTION, None, None, 500)
    }

    /// Loads every stored comparison at or above `MIN_SCORE` as linking
    /// candidates (filtered server-side, so it stays small even as the full
    /// comparison cache grows to millions of rows).
    pub(crate) fn list_above_threshold_candidates(
        &self,
    ) -> Result<Vec<crate::matching::MatchCandidate>> {
        let filter = format!("score>={}", crate::matching::MIN_SCORE);
        let rows = self.list_all::<crate::store::types::ProviderMatchRow>(
            PROVIDER_PRODUCT_MATCHES_COLLECTION,
            Some(&filter),
            None,
            500,
        )?;
        Ok(rows
            .into_iter()
            .map(|r| crate::matching::MatchCandidate {
                provider_product_id: r.provider_product_id,
                product_id: r.product_id,
                score: r.score,
            })
            .collect())
    }

    /// Resolves the latest price per provider product by listing prices sorted
    /// newest-first and keeping the first row seen for each provider product.
    pub(crate) fn latest_price_per_provider_product(&self) -> Result<HashMap<String, f64>> {
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

    /// Writes `brand_id` on a provider product (only called when it changed).
    pub(crate) fn update_brand_link(
        &self,
        provider_product_id: &str,
        brand_id: Option<&str>,
    ) -> Result<()> {
        self.client
            .records(PROVIDER_PRODUCTS_COLLECTION)
            .update(
                provider_product_id,
                BrandLinkPayload {
                    brand_id: brand_id.map(str::to_owned),
                },
            )
            .call()
            .map_err(|e| anyhow::anyhow!("could not update brand link: {e}"))
            .map(|_| ())
    }
}