use anyhow::Result;

use crate::domain::model::Matrix;

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

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::domain::model::{MatrixProvider, MatrixRow};

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
