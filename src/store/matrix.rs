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

        let providers = sort_providers_by_price_count(self.list_providers()?, &rows);

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

impl Matrix {
    /// Serializes the matrix as CSV with the same table structure as
    /// `GET /matrix`: one column per provider (header = domain), one row per
    /// product, raw numeric prices, and a blank cell when a provider doesn't
    /// carry the product. A UTF-8 BOM is prepended so Excel detects the
    /// encoding.
    pub fn to_csv(&self) -> Result<String> {
        let mut writer = csv::Writer::from_writer(Vec::new());
        let mut header = vec!["Product".to_string()];
        header.extend(self.providers.iter().map(|p| p.domain.clone()));
        writer.write_record(&header)?;
        for row in &self.rows {
            let mut record = vec![row.name.clone()];
            record.extend(self.providers.iter().map(|provider| {
                row.prices
                    .get(&provider.id)
                    .map(|price| price.to_string())
                    .unwrap_or_default()
            }));
            writer.write_record(&record)?;
        }
        writer.flush()?;
        let bytes = writer.into_inner().map_err(|e| {
            anyhow::anyhow!("could not finalize CSV export: {e}")
        })?;
        let mut csv = String::from_utf8(bytes)
            .map_err(|e| anyhow::anyhow!("CSV export is not valid UTF-8: {e}"))?;
        csv.insert(0, '\u{feff}');
        Ok(csv)
    }
}

#[cfg(test)]
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

    #[test]
    fn to_csv_writes_table_matching_matrix_structure() {
        let mut p1 = HashMap::new();
        p1.insert("prov-a".to_string(), 242100.0);
        let mut p2 = HashMap::new();
        p2.insert("prov-a".to_string(), 242100.0);
        p2.insert("prov-b".to_string(), 253000.5);
        let matrix = Matrix {
            generated_at: "2026-08-13 00:00:00.000Z".to_string(),
            providers: vec![
                MatrixProvider {
                    id: "prov-a".to_string(),
                    domain: "a.com.ar".to_string(),
                    name: "a".to_string(),
                },
                MatrixProvider {
                    id: "prov-b".to_string(),
                    domain: "b.com.ar".to_string(),
                    name: "b".to_string(),
                },
            ],
            rows: vec![
                MatrixRow {
                    product_id: "prod-1".to_string(),
                    name: "Alfa EDP 50 ml".to_string(),
                    prices: p1,
                },
                MatrixRow {
                    product_id: "prod-2".to_string(),
                    name: "Beta EDP 100 ml".to_string(),
                    prices: p2,
                },
            ],
        };
        let csv = matrix.to_csv().unwrap();
        assert!(csv.starts_with('\u{feff}'));
        let body = csv.trim_start_matches('\u{feff}');
        assert_eq!(
            body,
            "Product,a.com.ar,b.com.ar\n\
             Alfa EDP 50 ml,242100,\n\
             Beta EDP 100 ml,242100,253000.5\n"
        );
    }
}
