//! Pure CSV tables for spreadsheet consumption (no `csv`/`anyhow`).
//! Builds RFC-4180 CSV text manually so `price_hunter_domain` stays free of
//! I/O crates. The tables here are the source of truth for ordering, folding
//! and escaping; infrastructure writes them straight to disk.

use crate::model::{BrandRow, Matrix, ProductRow};
use crate::text::ascii_fold;

/// Escapes one CSV field per RFC-4180: fields containing `,`, `"`, `\n` or
/// `\r` are wrapped in quotes with `"` doubled.
fn escape_field(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn tables_to_csv(header: &[String], rows: &[Vec<String>]) -> String {
    let mut out = String::from('\u{feff}');
    out.push_str(
        &header
            .iter()
            .map(|h| escape_field(h))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push('\n');
    for row in rows {
        out.push_str(
            &row.iter()
                .map(|c| escape_field(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    out
}

/// Serializes the matrix as CSV with the same table structure as
/// `GET /matrix`: one column per provider (header = domain), one row per
/// product, raw numeric prices, and a blank cell when a provider doesn't
/// carry the product. A UTF-8 BOM is prepended so Excel detects the
/// encoding.
pub fn matrix_to_csv(matrix: &Matrix) -> String {
    let header: Vec<String> = std::iter::once("Product".to_string())
        .chain(matrix.providers.iter().map(|p| p.domain.clone()))
        .collect();
    let rows: Vec<Vec<String>> = matrix
        .rows
        .iter()
        .map(|row| {
            let mut record = vec![row.name.clone()];
            record.extend(matrix.providers.iter().map(|provider| {
                row.prices
                    .get(&provider.id)
                    .map(|price| price.to_string())
                    .unwrap_or_default()
            }));
            record
        })
        .collect();
    tables_to_csv(&header, &rows)
}

/// Serializes the canonical products as CSV with `brand,product_name`
/// columns, one row per product sorted by brand, then product_name.
/// All values are lowercased and folded to ASCII. A UTF-8 BOM is prepended so
/// Excel detects the encoding.
pub fn products_to_csv(products: &[ProductRow]) -> String {
    let mut order: Vec<usize> = (0..products.len()).collect();
    order.sort_by(|&a, &b| {
        let pa = &products[a];
        let pb = &products[b];
        ascii_fold(&pa.brand)
            .cmp(&ascii_fold(&pb.brand))
            .then_with(|| ascii_fold(&pa.product_name).cmp(&ascii_fold(&pb.product_name)))
    });
    let header = vec!["brand".to_string(), "product_name".to_string()];
    let rows: Vec<Vec<String>> = order
        .into_iter()
        .map(|i| {
            let product = &products[i];
            vec![
                ascii_fold(&product.brand),
                ascii_fold(&product.product_name),
            ]
        })
        .collect();
    tables_to_csv(&header, &rows)
}

/// Serializes the canonical brands as CSV with a single `name` column, one
/// row per brand sorted by name. Values are lowercased and folded to ASCII. A
/// UTF-8 BOM is prepended so Excel detects the encoding.
pub fn brands_to_csv(brands: &[BrandRow]) -> String {
    let mut order: Vec<usize> = (0..brands.len()).collect();
    order.sort_by(|&a, &b| ascii_fold(&brands[a].name).cmp(&ascii_fold(&brands[b].name)));
    let header = vec!["name".to_string()];
    let rows: Vec<Vec<String>> = order
        .into_iter()
        .map(|i| vec![ascii_fold(&brands[i].name)])
        .collect();
    tables_to_csv(&header, &rows)
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::model::{MatrixProvider, MatrixRow};

    #[test]
    fn brands_to_csv_writes_single_name_column_sorted_and_folded() {
        let brands = vec![
            BrandRow {
                id: "b3".to_string(),
                name: "Bambú".to_string(),
            },
            BrandRow {
                id: "b2".to_string(),
                name: "zeta".to_string(),
            },
            BrandRow {
                id: "b1".to_string(),
                name: "Alfa".to_string(),
            },
        ];
        let csv = brands_to_csv(&brands);
        assert!(csv.starts_with('\u{feff}'));
        let body = csv.trim_start_matches('\u{feff}');
        assert_eq!(body, "name\nalfa\nbambu\nzeta\n");
    }

    #[test]
    fn products_to_csv_sorts_by_brand_product_name_and_folds_ascii() {
        let products = vec![
            ProductRow {
                id: "p3".to_string(),
                name: "Zeta EDP 100 ml".to_string(),
                brand: "Zeta".to_string(),
                product_name: "EDP 100".to_string(),
            },
            ProductRow {
                id: "p2".to_string(),
                name: "Alfa EDP 50 ml".to_string(),
                brand: "alfa".to_string(),
                product_name: "EDP 50".to_string(),
            },
            ProductRow {
                id: "p4".to_string(),
                name: "Alfa Shower Gel 250 ml".to_string(),
                brand: "Alfa".to_string(),
                product_name: "Shower Gel".to_string(),
            },
            ProductRow {
                id: "p1".to_string(),
                name: "Alfa EDP 50 ml".to_string(),
                brand: "ALFA".to_string(),
                product_name: "EDP 50".to_string(),
            },
            ProductRow {
                id: "p6".to_string(),
                name: "Adolfo Dominguez Agua de Bambú EDT 120 ml".to_string(),
                brand: "adolfo dominguez".to_string(),
                product_name: "Agua de Bambú EDT".to_string(),
            },
            ProductRow {
                id: "p5".to_string(),
                name: "Adolfo Dominguez Agua de Bambu Man EDP 200 ml".to_string(),
                brand: "Adolfo Dominguez".to_string(),
                product_name: "Agua de Bambu Man EDP".to_string(),
            },
        ];
        let csv = products_to_csv(&products);
        assert!(csv.starts_with('\u{feff}'));
        let body = csv.trim_start_matches('\u{feff}');
        assert_eq!(
            body,
            "brand,product_name\n\
             adolfo dominguez,agua de bambu edt\n\
             adolfo dominguez,agua de bambu man edp\n\
             alfa,edp 50\n\
             alfa,edp 50\n\
             alfa,shower gel\n\
             zeta,edp 100\n"
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
                    size: "50 ml".to_string(),
                    name: "Alfa EDP 50 ml".to_string(),
                    prices: p1,
                },
                MatrixRow {
                    product_id: "prod-2".to_string(),
                    size: "100 ml".to_string(),
                    name: "Beta EDP 100 ml".to_string(),
                    prices: p2,
                },
            ],
        };
        let csv = matrix_to_csv(&matrix);
        assert!(csv.starts_with('\u{feff}'));
        let body = csv.trim_start_matches('\u{feff}');
        assert_eq!(
            body,
            "Product,a.com.ar,b.com.ar\n\
             Alfa EDP 50 ml,242100,\n\
             Beta EDP 100 ml,242100,253000.5\n"
        );
    }

    #[test]
    fn escape_field_quotes_commas_and_quotes() {
        assert_eq!(escape_field("plain"), "plain");
        assert_eq!(escape_field("a,b"), "\"a,b\"");
        assert_eq!(escape_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }
}
