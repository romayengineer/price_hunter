use std::collections::HashMap;

use anyhow::Result;

use crate::domain::model::{
    Matrix, MatrixProvider, MatrixRow, ProductRow, ProviderProductRow, ProviderRow,
};
use crate::domain::ports::PriceStore;
use crate::domain::time::{iso8601, now_secs};

/// Builds the product × provider price matrix: one row per product with a
/// price at two or more distinct providers, one column per provider, and
/// the latest scraped price in each cell. When a product maps to several
/// listings on the same provider the lowest price wins.
pub fn matrix(store: &impl PriceStore) -> Result<Matrix> {
    let products = store.list_all_products()?;
    let provider_products = store.list_provider_products()?;
    let latest_prices = store.latest_price_per_provider_product()?;

    let mut rows = matrix_rows(&products, &provider_products, &latest_prices);
    rows.sort_by_key(|r| r.name.to_lowercase());

    let providers = sort_providers_by_price_count(store.list_providers()?, &rows);

    Ok(Matrix {
        generated_at: iso8601(now_secs()),
        providers,
        rows,
    })
}

/// Orders provider columns so the ones with the most priced products come
/// first (most non-blank cells on the left), with ties broken by domain for a
/// stable order. Providers with no prices still appear, on the right.
fn sort_providers_by_price_count(
    mut providers: Vec<ProviderRow>,
    rows: &[MatrixRow],
) -> Vec<MatrixProvider> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for row in rows {
        for id in row.prices.keys() {
            *counts.entry(id.as_str()).or_default() += 1;
        }
    }
    providers.sort_by(|a, b| {
        let ca = counts.get(a.id.as_str()).copied().unwrap_or(0);
        let cb = counts.get(b.id.as_str()).copied().unwrap_or(0);
        cb.cmp(&ca).then_with(|| a.domain.cmp(&b.domain))
    });
    providers
        .into_iter()
        .map(|p| MatrixProvider {
            id: p.id,
            domain: p.domain,
            name: p.name,
        })
        .collect()
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

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;

    #[test]
    fn sort_providers_by_price_count_orders_most_priced_left() {
        let providers = vec![
            ProviderRow {
                id: "p3".to_string(),
                domain: "c.com".to_string(),
                name: "c".to_string(),
                enabled: true,
                default_currency: None,
            },
            ProviderRow {
                id: "p2".to_string(),
                domain: "b.com".to_string(),
                name: "b".to_string(),
                enabled: true,
                default_currency: None,
            },
            ProviderRow {
                id: "p1".to_string(),
                domain: "a.com".to_string(),
                name: "a".to_string(),
                enabled: true,
                default_currency: None,
            },
            ProviderRow {
                id: "p4".to_string(),
                domain: "d.com".to_string(),
                name: "d".to_string(),
                enabled: true,
                default_currency: None,
            },
        ];
        let rows = vec![
            MatrixRow {
                product_id: "x".to_string(),
                name: "X".to_string(),
                prices: HashMap::from([("p1".to_string(), 1.0), ("p2".to_string(), 2.0)]),
            },
            MatrixRow {
                product_id: "y".to_string(),
                name: "Y".to_string(),
                prices: HashMap::from([
                    ("p1".to_string(), 3.0),
                    ("p2".to_string(), 4.0),
                    ("p3".to_string(), 5.0),
                ]),
            },
            MatrixRow {
                product_id: "z".to_string(),
                name: "Z".to_string(),
                prices: HashMap::from([("p2".to_string(), 6.0)]),
            },
        ];
        // counts: p2=3, p1=2, p4=0, p3=1 -> p2, p1, p3, p4 (domain tie-break none)
        let sorted = sort_providers_by_price_count(providers, &rows);
        let domains: Vec<&str> = sorted.iter().map(|p| p.domain.as_str()).collect();
        assert_eq!(domains, vec!["b.com", "a.com", "c.com", "d.com"]);
    }

    #[test]
    fn sort_providers_by_price_count_breaks_ties_by_domain() {
        let providers = vec![
            ProviderRow {
                id: "p1".to_string(),
                domain: "b.com".to_string(),
                name: "b".to_string(),
                enabled: true,
                default_currency: None,
            },
            ProviderRow {
                id: "p2".to_string(),
                domain: "a.com".to_string(),
                name: "a".to_string(),
                enabled: true,
                default_currency: None,
            },
        ];
        let rows = vec![MatrixRow {
            product_id: "x".to_string(),
            name: "X".to_string(),
            prices: HashMap::from([("p1".to_string(), 1.0), ("p2".to_string(), 2.0)]),
        }];
        let sorted = sort_providers_by_price_count(providers, &rows);
        let domains: Vec<&str> = sorted.iter().map(|p| p.domain.as_str()).collect();
        assert_eq!(domains, vec!["a.com", "b.com"]);
    }
}
