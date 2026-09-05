//! Use case: pure persistence decisions for scraped products (currency,
//! size, price-change and image diffs). All HTTP stays in infrastructure.

use std::collections::{HashMap, HashSet};

use crate::matching::split_size;

/// Resolves the currency for a scraped product: card currency wins, then the
/// provider default, otherwise empty.
pub fn resolve_currency(
    product_currency: Option<&str>,
    provider_default: Option<&str>,
) -> String {
    product_currency
        .map(str::to_owned)
        .or_else(|| provider_default.map(str::to_owned))
        .unwrap_or_default()
}

/// The normalized size for a scraped product name (empty when unknown).
pub fn product_size(name: &str) -> String {
    split_size(name).1.unwrap_or_default()
}

/// Whether the freshly scraped price equals the last recorded one (same value
/// and currency) and so no new price row is needed.
pub fn price_unchanged(
    last_price: f64,
    last_currency: &str,
    next_price: f64,
    next_currency: &str,
) -> bool {
    last_price == next_price && last_currency == next_currency
}

/// Image URLs in `next` that have no matching row in `existing` and need an
/// insert (position = index in `next`; position 0 is primary).
pub fn missing_images(existing: &[String], next: &[String]) -> Vec<(usize, String)> {
    let known: HashSet<&str> = existing.iter().map(String::as_str).collect();
    next.iter()
        .enumerate()
        .filter(|(_, url)| !known.contains(url.as_str()))
        .map(|(position, url)| (position, url.clone()))
        .collect()
}

/// Existing image URLs absent from `next` whose rows should be deleted.
pub fn stale_images(existing: &[String], next: &[String]) -> Vec<String> {
    let wanted: HashSet<&str> = next.iter().map(String::as_str).collect();
    existing
        .iter()
        .filter(|url| !wanted.contains(url.as_str()))
        .cloned()
        .collect()
}

/// Folds `(provider_product_id, price)` rows sorted `-created` (newest first)
/// into the latest price per provider product (first wins).
pub fn fold_latest_prices(
    rows: impl IntoIterator<Item = (String, f64)>,
) -> HashMap<String, f64> {
    let mut prices = HashMap::new();
    for (id, price) in rows {
        prices.entry(id).or_insert(price);
    }
    prices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn currency_prefers_card_then_provider() {
        assert_eq!(resolve_currency(Some("ARS"), Some("USD")), "ARS");
        assert_eq!(resolve_currency(None, Some("USD")), "USD");
        assert_eq!(resolve_currency(None, None), "");
    }

    #[test]
    fn size_splits_trailing_unit() {
        assert_eq!(product_size("Cool EDT 100 ml"), "100 ml");
        assert_eq!(product_size("Cool EDT"), "");
    }

    #[test]
    fn price_guard_matches_value_and_currency() {
        assert!(price_unchanged(10.0, "ARS", 10.0, "ARS"));
        assert!(!price_unchanged(10.0, "ARS", 11.0, "ARS"));
        assert!(!price_unchanged(10.0, "ARS", 10.0, "USD"));
    }

    #[test]
    fn image_diffs_detect_missing_and_stale() {
        let existing = vec!["a".to_string()];
        let next = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            missing_images(&existing, &next),
            vec![(1, "b".to_string())]
        );
        assert!(stale_images(&existing, &next).is_empty());
        assert_eq!(stale_images(&next, &existing), vec!["b".to_string()]);
    }

    #[test]
    fn latest_fold_keeps_first() {
        let rows = vec![("p1".to_string(), 9.0), ("p1".to_string(), 10.0)];
        assert_eq!(fold_latest_prices(rows)["p1"], 9.0);
    }
}
