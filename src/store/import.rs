use anyhow::{Context, Result};

use super::Store;
use super::http::escape_filter;
use super::types::{PRODUCTS_COLLECTION, ProductImportPayload, ProductImportRow, RowOutcome};

impl Store {
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
}
