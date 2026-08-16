use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

use super::Store;

impl Store {
    /// Lists every record of `collection` with an optional filter and sort,
    /// paginating `per_page` rows at a time until the collection is exhausted.
    /// Keeps the page/per_page loop in one place instead of duplicating it in
    /// every bulk-loading path.
    pub(super) fn list_all<T>(
        &self,
        collection: &'static str,
        filter: Option<&str>,
        sort: Option<&str>,
        per_page: usize,
    ) -> Result<Vec<T>>
    where
        T: Default + DeserializeOwned,
    {
        let mut items = Vec::new();
        let mut page = 1;
        loop {
            let mut builder = self
                .client
                .records(collection)
                .list()
                .page(page)
                .per_page(per_page as i32);
            if let Some(filter) = filter {
                builder = builder.filter(filter);
            }
            if let Some(sort) = sort {
                builder = builder.sort(sort);
            }
            let result = builder
                .call::<T>()
                .with_context(|| format!("could not list {collection}"))?;
            let count = result.items.len();
            items.extend(result.items);
            if count < per_page {
                break;
            }
            page += 1;
        }
        Ok(items)
    }
}