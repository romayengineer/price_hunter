//! Use case: prune canonical `products` absent from a keep-list CSV
//! (`delete_products`). Pure set-diff and paging; the store and prompts stay
//! in infrastructure.

use std::collections::HashSet;

use crate::model::ProductRow;

/// Rows per confirmation page for bulk deletes.
pub const DELETE_PAGE_SIZE: usize = 50;

/// Number of confirmation pages for `total` stale rows.
pub fn page_count(total: usize) -> usize {
    total.div_ceil(DELETE_PAGE_SIZE)
}

/// Returns the canonical products whose `(brand, product_name)` is absent
/// from `keys`. `None` keys means delete everything.
pub fn stale_products(
    all: Vec<ProductRow>,
    keys: Option<&HashSet<(String, String)>>,
) -> Vec<ProductRow> {
    match keys {
        Some(keys) => all
            .into_iter()
            .filter(|p| !keys.contains(&(p.brand.clone(), p.product_name.clone())))
            .collect(),
        None => all,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, brand: &str, name: &str) -> ProductRow {
        ProductRow {
            id: id.to_string(),
            name: name.to_string(),
            brand: brand.to_string(),
            product_name: name.to_string(),
        }
    }

    #[test]
    fn keeps_rows_present_in_csv() {
        let all = vec![row("1", "diesel", "fuel"), row("2", "adidas", "vibes")];
        let keys = HashSet::from([("diesel".to_string(), "fuel".to_string())]);
        let stale = stale_products(all, Some(&keys));
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].id, "2");
    }

    #[test]
    fn none_keys_deletes_everything() {
        let all = vec![row("1", "a", "b")];
        assert_eq!(stale_products(all, None).len(), 1);
    }

    #[test]
    fn page_count_rounds_up() {
        assert_eq!(page_count(0), 0);
        assert_eq!(page_count(50), 1);
        assert_eq!(page_count(51), 2);
    }
}
