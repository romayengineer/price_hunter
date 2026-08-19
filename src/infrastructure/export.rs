//! Serializes the price matrix and the canonical products as CSV for
//! spreadsheet consumption.

use anyhow::Result;

use crate::domain::model::{BrandRow, Matrix, ProductRow};

/// Lowercases `s` and replaces non-ASCII characters with their closest ASCII
/// match: accented Latin letters lose their diacritics (`bambú` → `bambu`,
/// `Benoît` → `benoit`), curly quotes and acute accents become `'`, and zero-
/// width / BOM characters are dropped. Characters with no ASCII equivalent are
/// left unchanged. Used both as the sort key and for the CSV output, so a
/// lowercase, ASCII-only file round-trips deterministically and sorts by the
/// base letters (`bambú` and `bambu` compare equal, then the rest of the name
/// decides the order).
fn ascii_fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        fold_char(c, &mut out);
    }
    out
}

/// Appends the lowercase ASCII fold of `c` (possibly several chars, e.g.
/// `ß` → `ss`, or none for zero-width marks) to `out`.
fn fold_char(c: char, out: &mut String) {
    match c {
        '\u{feff}' | '\u{200b}' | '\u{200c}' | '\u{200d}' => {}
        c if c.is_ascii() => out.push(c.to_ascii_lowercase()),
        c => match c.to_lowercase().to_string().as_str() {
            "à" | "á" | "â" | "ã" | "ä" | "å" => out.push('a'),
            "è" | "é" | "ê" | "ë" => out.push('e'),
            "ì" | "í" | "î" | "ï" => out.push('i'),
            "ò" | "ó" | "ô" | "õ" | "ö" | "ø" => out.push('o'),
            "ù" | "ú" | "û" | "ü" => out.push('u'),
            "ñ" => out.push('n'),
            "ç" => out.push('c'),
            "ß" => out.push_str("ss"),
            "ÿ" => out.push('y'),
            "æ" => out.push_str("ae"),
            "œ" => out.push_str("oe"),
            "\u{00b4}" | "\u{2018}" | "\u{2019}" | "\u{201a}" | "\u{201b}" | "\u{02b9}"
            | "\u{02bc}" => out.push('\''),
            _ => out.push(c),
        },
    }
}

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
/// columns, one row per product sorted by brand, then product_name, then size.
/// All values are lowercased and folded to ASCII. A UTF-8 BOM is prepended so
/// Excel detects the encoding.
pub fn products_to_csv(products: &[ProductRow]) -> Result<String> {
    let mut order: Vec<usize> = (0..products.len()).collect();
    order.sort_by(|&a, &b| {
        let pa = &products[a];
        let pb = &products[b];
        ascii_fold(&pa.brand)
            .cmp(&ascii_fold(&pb.brand))
            .then_with(|| ascii_fold(&pa.product_name).cmp(&ascii_fold(&pb.product_name)))
            .then_with(|| ascii_fold(&pa.size).cmp(&ascii_fold(&pb.size)))
    });
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(["brand", "product_name", "size"])?;
    for i in order {
        let product = &products[i];
        writer.write_record([
            ascii_fold(&product.brand).as_str(),
            ascii_fold(&product.product_name).as_str(),
            ascii_fold(&product.size).as_str(),
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

/// Serializes the canonical brands as CSV with a single `name` column, one
/// row per brand sorted by name. Values are lowercased and folded to ASCII. A
/// UTF-8 BOM is prepended so Excel detects the encoding.
pub fn brands_to_csv(brands: &[BrandRow]) -> Result<String> {
    let mut order: Vec<usize> = (0..brands.len()).collect();
    order.sort_by(|&a, &b| ascii_fold(&brands[a].name).cmp(&ascii_fold(&brands[b].name)));
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(["name"])?;
    for i in order {
        writer.write_record([ascii_fold(&brands[i].name).as_str()])?;
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
        let csv = brands_to_csv(&brands).unwrap();
        assert!(csv.starts_with('\u{feff}'));
        let body = csv.trim_start_matches('\u{feff}');
        assert_eq!(body, "name\nalfa\nbambu\nzeta\n");
    }

    #[test]
    fn products_to_csv_sorts_by_brand_product_name_size_and_folds_ascii() {
        let products = vec![
            ProductRow {
                id: "p3".to_string(),
                name: "Zeta EDP 100 ml".to_string(),
                brand: "Zeta".to_string(),
                product_name: "EDP 100".to_string(),
                size: "100 ml".to_string(),
            },
            ProductRow {
                id: "p2".to_string(),
                name: "Alfa EDP 50 ml".to_string(),
                brand: "alfa".to_string(),
                product_name: "EDP 50".to_string(),
                size: "50 ml".to_string(),
            },
            ProductRow {
                id: "p4".to_string(),
                name: "Alfa Shower Gel 250 ml".to_string(),
                brand: "Alfa".to_string(),
                product_name: "Shower Gel".to_string(),
                size: "250 ml".to_string(),
            },
            ProductRow {
                id: "p1".to_string(),
                name: "Alfa EDP 50 ml".to_string(),
                brand: "ALFA".to_string(),
                product_name: "EDP 50".to_string(),
                size: "30 ml".to_string(),
            },
            ProductRow {
                id: "p6".to_string(),
                name: "Adolfo Dominguez Agua de Bambú EDT 120 ml".to_string(),
                brand: "adolfo dominguez".to_string(),
                product_name: "Agua de Bambú EDT".to_string(),
                size: "120 ml".to_string(),
            },
            ProductRow {
                id: "p5".to_string(),
                name: "Adolfo Dominguez Agua de Bambu Man EDP 200 ml".to_string(),
                brand: "Adolfo Dominguez".to_string(),
                product_name: "Agua de Bambu Man EDP".to_string(),
                size: "200 ml".to_string(),
            },
        ];
        let csv = products_to_csv(&products).unwrap();
        assert!(csv.starts_with('\u{feff}'));
        let body = csv.trim_start_matches('\u{feff}');
        assert_eq!(
            body,
            "brand,product_name,size\n\
             adolfo dominguez,agua de bambu edt,120 ml\n\
             adolfo dominguez,agua de bambu man edp,200 ml\n\
             alfa,edp 50,30 ml\n\
             alfa,edp 50,50 ml\n\
             alfa,shower gel,250 ml\n\
             zeta,edp 100,100 ml\n"
        );
    }

    #[test]
    fn ascii_fold_lowercases_and_transliterates() {
        assert_eq!(ascii_fold("Bambú"), "bambu");
        assert_eq!(ascii_fold("Agua de Bambú EDT"), "agua de bambu edt");
        assert_eq!(ascii_fold("Benoît"), "benoit");
        assert_eq!(ascii_fold("José Ñoño"), "jose nono");
        assert_eq!(ascii_fold("François Straße"), "francois strasse");
        assert_eq!(ascii_fold("A’B"), "a'b");
        assert_eq!(ascii_fold("\u{feff}abc\u{200b}"), "abc");
        assert_eq!(ascii_fold("30° C"), "30° c");
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
