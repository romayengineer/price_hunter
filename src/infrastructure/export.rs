//! Serializes the price matrix and the canonical products as CSV for
//! spreadsheet consumption.

use anyhow::Result;

use crate::domain::model::{BrandRow, Matrix, ProductRow};

/// Serializes the matrix as CSV with the same table structure as
/// `GET /matrix`: one column per provider (header = domain), one row per
/// product, raw numeric prices, and a blank cell when a provider doesn't
/// carry the product. A UTF-8 BOM is prepended so Excel detects the
/// encoding.
pub fn matrix_to_csv(matrix: &Matrix) -> Result<String> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    let mut header = vec!["Product".to_string()];
    header.extend(matrix.providers.iter().map(|p| p.domain.clone()));
    writer.write_record(&header)?;
    for row in &matrix.rows {
        let mut record = vec![row.name.clone()];
        record.extend(matrix.providers.iter().map(|provider| {
            row.prices
                .get(&provider.id)
                .map(|price| price.to_string())
                .unwrap_or_default()
        }));
        writer.write_record(&record)?;
    }
    writer.flush()?;
    let bytes = writer
        .into_inner()
        .map_err(|e| anyhow::anyhow!("could not finalize CSV export: {e}"))?;
    let mut csv = String::from_utf8(bytes)
        .map_err(|e| anyhow::anyhow!("CSV export is not valid UTF-8: {e}"))?;
    csv.insert(0, '\u{feff}');
    Ok(csv)
}

/// Serializes the canonical products as CSV with `brand,product_name,size`
/// columns, one row per product sorted by display name. A UTF-8 BOM is
/// prepended so Excel detects the encoding.
pub fn products_to_csv(products: &[ProductRow]) -> Result<String> {
    let mut order: Vec<usize> = (0..products.len()).collect();
    order.sort_by(|&a, &b| {
        products[a]
            .name
            .to_lowercase()
            .cmp(&products[b].name.to_lowercase())
    });
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(["brand", "product_name", "size"])?;
    for i in order {
        let product = &products[i];
        writer.write_record([
            product.brand.as_str(),
            product.product_name.as_str(),
            product.size.as_str(),
        ])?;
    }
    writer.flush()?;
    let bytes = writer
        .into_inner()
        .map_err(|e| anyhow::anyhow!("could not finalize CSV export: {e}"))?;
    let mut csv = String::from_utf8(bytes)
        .map_err(|e| anyhow::anyhow!("CSV export is not valid UTF-8: {e}"))?;
    csv.insert(0, '\u{feff}');
    Ok(csv)
}

/// Serializes the canonical brands as CSV with a single `brand` column, one
/// row per brand sorted by name. A UTF-8 BOM is prepended so Excel detects
/// the encoding.
pub fn brands_to_csv(brands: &[BrandRow]) -> Result<String> {
    let mut order: Vec<usize> = (0..brands.len()).collect();
    order.sort_by(|&a, &b| brands[a].name.to_lowercase().cmp(&brands[b].name.to_lowercase()));
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(["brand"])?;
    for i in order {
        writer.write_record([brands[i].name.as_str()])?;
    }
    writer.flush()?;
    let bytes = writer
        .into_inner()
        .map_err(|e| anyhow::anyhow!("could not finalize CSV export: {e}"))?;
    let mut csv = String::from_utf8(bytes)
        .map_err(|e| anyhow::anyhow!("CSV export is not valid UTF-8: {e}"))?;
    csv.insert(0, '\u{feff}');
    Ok(csv)
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::domain::model::{MatrixProvider, MatrixRow};

    #[test]
    fn brands_to_csv_writes_single_brand_column_sorted() {
        let brands = vec![
            BrandRow {
                id: "b2".to_string(),
                name: "zeta".to_string(),
            },
            BrandRow {
                id: "b1".to_string(),
                name: "Alfa".to_string(),
            },
        ];
        let csv = brands_to_csv(&brands).unwrap();
        assert!(csv.starts_with('\u{feff}'));
        let body = csv.trim_start_matches('\u{feff}');
        assert_eq!(body, "brand\nAlfa\nzeta\n");
    }

    #[test]
    fn products_to_csv_writes_brand_product_name_and_size_sorted() {
        let products = vec![
            ProductRow {
                id: "p2".to_string(),
                name: "Zeta EDP 100 ml".to_string(),
                brand: "zeta".to_string(),
                product_name: "EDP 100".to_string(),
                size: "100 ml".to_string(),
            },
            ProductRow {
                id: "p1".to_string(),
                name: "Alfa EDP 50 ml".to_string(),
                brand: "alfa".to_string(),
                product_name: "EDP 50".to_string(),
                size: "50 ml".to_string(),
            },
        ];
        let csv = products_to_csv(&products).unwrap();
        assert!(csv.starts_with('\u{feff}'));
        let body = csv.trim_start_matches('\u{feff}');
        assert_eq!(
            body,
            "brand,product_name,size\n\
             alfa,EDP 50,50 ml\n\
             zeta,EDP 100,100 ml\n"
        );
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
        let csv = matrix_to_csv(&matrix).unwrap();
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
