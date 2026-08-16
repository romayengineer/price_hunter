use std::collections::HashMap;

use anyhow::{Context, Result};

use super::Store;
use super::http::{iso8601, now_secs};
use super::types::{
    PRODUCTS_COLLECTION, PROVIDER_PRODUCT_PRICES_COLLECTION,
    PROVIDERS_COLLECTION, Matrix, MatrixProvider, MatrixRow, ProductRow, ProviderPriceRow,
    ProviderProductRow, ProviderRow,
};

impl Store {
    /// Builds the product × provider price matrix: one row per product with a
    /// price at two or more distinct providers, one column per provider, and
    /// the latest scraped price in each cell. When a product maps to several
    /// listings on the same provider the lowest price wins.
    pub fn matrix(&self) -> Result<Matrix> {
        let products = self.list_all_products()?;
        let provider_products = self.list_provider_products()?;
        let latest_prices = self.latest_price_per_provider_product()?;

        let mut rows = matrix_rows(&products, &provider_products, &latest_prices);
        rows.sort_by_key(|r| r.name.to_lowercase());

        let mut providers = self.list_providers()?;
        providers.sort_by_key(|p| p.domain.clone());
        let providers = providers
            .into_iter()
            .map(|p| MatrixProvider {
                id: p.id,
                domain: p.domain,
                name: p.name,
            })
            .collect();

        Ok(Matrix {
            generated_at: iso8601(now_secs()),
            providers,
            rows,
        })
    }

    /// Lists every canonical product (no `active` filter — the matrix includes
    /// retired products that still have listings).
    fn list_all_products(&self) -> Result<Vec<ProductRow>> {
        let mut items = Vec::new();
        let mut page = 1;
        loop {
            let result = self
                .client
                .records(PRODUCTS_COLLECTION)
                .list()
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

    /// Lists every provider.
    fn list_providers(&self) -> Result<Vec<ProviderRow>> {
        let mut items = Vec::new();
        let mut page = 1;
        loop {
            let result = self
                .client
                .records(PROVIDERS_COLLECTION)
                .list()
                .page(page)
                .per_page(100)
                .call::<ProviderRow>()
                .context("could not list providers")?;
            let count = result.items.len();
            items.extend(result.items);
            if count < 100 {
                break;
            }
            page += 1;
        }
        Ok(items)
    }

    /// Resolves the latest price per provider product by listing prices sorted
    /// newest-first and keeping the first row seen for each provider product.
    fn latest_price_per_provider_product(&self) -> Result<HashMap<String, f64>> {
        let mut prices = HashMap::new();
        let mut page = 1;
        loop {
            let result = self
                .client
                .records(PROVIDER_PRODUCT_PRICES_COLLECTION)
                .list()
                .sort("-created")
                .page(page)
                .per_page(500)
                .call::<ProviderPriceRow>()
                .context("could not list prices")?;
            let count = result.items.len();
            for row in result.items {
                prices.entry(row.provider_product_id).or_insert(row.price);
            }
            if count < 500 {
                break;
            }
            page += 1;
        }
        Ok(prices)
    }
}

/// Builds one matrix row per product that has a price at two or more distinct
/// providers, with the latest price per provider (lowest when a product maps
/// to several listings on the same provider).
fn matrix_rows(
    products: &[ProductRow],
    provider_products: &[ProviderProductRow],
    latest_prices: &HashMap<String, f64>,
) -> Vec<MatrixRow> {
    let mut rows = Vec::new();
    for product in products {
        let prices = cell_prices(provider_products, &product.id, latest_prices);
        if prices.len() < 2 {
            continue;
        }
        rows.push(MatrixRow {
            product_id: product.id.clone(),
            name: product.name.clone(),
            prices,
        });
    }
    rows
}

/// Latest price per provider for one product, taking the lowest when the
/// product maps to several listings on the same provider.
fn cell_prices(
    provider_products: &[ProviderProductRow],
    product_id: &str,
    latest_prices: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    provider_products
        .iter()
        .filter(|pp| pp.product_id.as_deref() == Some(product_id))
        .filter_map(|pp| {
            latest_prices
                .get(&pp.id)
                .map(|price| (pp.provider_id.clone(), *price))
        })
        .fold(HashMap::new(), |mut prices, (provider, price)| {
            let best = prices.entry(provider).or_insert(price);
            *best = best.min(price);
            prices
        })
}
