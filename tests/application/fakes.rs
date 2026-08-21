//! In-memory [`PriceStore`] fake used by the application use-case tests. It
//! holds the same normalized tables the PocketBase adapter persists and records
//! every write, so tests can assert on the outcome without any I/O.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use price_hunter::domain::error::PriceStoreError;
use price_hunter::domain::matching::{MIN_SCORE, MatchCandidate};
use price_hunter::domain::model::{
    BrandRow, MatchInsert, ProductInsert, ProductRow, ProviderMatchRow, ProviderProductRow,
    ProviderRow,
};
use price_hunter::domain::ports::{
    BrandCatalog, MatchStore, PriceHistory, ProductCatalog, ProviderCatalog,
};

/// In-memory [`PriceStore`] for exercising the application use cases offline.
#[derive(Default)]
pub struct FakeStore {
    pub products: Vec<ProductRow>,
    pub provider_products: Vec<ProviderProductRow>,
    pub providers: Vec<ProviderRow>,
    pub brands: Vec<BrandRow>,
    pub latest_prices: HashMap<String, f64>,
    /// When `true` every method returns a typed store error (error propagation).
    pub fail: bool,
    matches: RefCell<Vec<ProviderMatchRow>>,
    product_links: RefCell<HashMap<String, Option<String>>>,
    brand_links: RefCell<HashMap<String, Option<String>>>,
    deleted_provider_products: RefCell<Vec<String>>,
    created_products: RefCell<Vec<(String, String, String, String)>>,
}

impl FakeStore {
    fn check(&self) -> Result<(), PriceStoreError> {
        if self.fail {
            Err(PriceStoreError::Request("boom".to_string()))
        } else {
            Ok(())
        }
    }

    /// Records one pre-existing comparison (e.g. from a previous run).
    pub fn seed_match(&self, provider_product_id: &str, product_id: &str, score: f64) {
        let n = self.matches.borrow().len();
        self.matches.borrow_mut().push(ProviderMatchRow {
            id: format!("match-{n}"),
            provider_product_id: provider_product_id.to_string(),
            product_id: product_id.to_string(),
            score,
            status: "pending".to_string(),
        });
    }

    /// Records the latest price for a provider product.
    pub fn seed_price(&mut self, provider_product_id: &str, price: f64) {
        self.latest_prices
            .insert(provider_product_id.to_string(), price);
    }

    /// The stored comparison rows, for assertions.
    pub fn matches(&self) -> Vec<ProviderMatchRow> {
        self.matches
            .borrow()
            .iter()
            .map(|m| ProviderMatchRow {
                id: m.id.clone(),
                provider_product_id: m.provider_product_id.clone(),
                product_id: m.product_id.clone(),
                score: m.score,
                status: m.status.clone(),
            })
            .collect()
    }

    /// The recorded `provider_product_id -> product_id` links.
    pub fn product_links(&self) -> HashMap<String, Option<String>> {
        self.product_links.borrow().clone()
    }

    /// The recorded `provider_product_id -> brand_id` assignments.
    pub fn brand_links(&self) -> HashMap<String, Option<String>> {
        self.brand_links.borrow().clone()
    }

    /// The provider products deleted via `delete_provider_product`.
    pub fn deleted_provider_products(&self) -> Vec<String> {
        self.deleted_provider_products.borrow().clone()
    }

    /// The canonical products created via `create_product`, as
    /// `(brand, product_name, size, name)` tuples.
    pub fn created_products(&self) -> Vec<(String, String, String, String)> {
        self.created_products.borrow().clone()
    }
}

impl ProductCatalog for FakeStore {
    fn list_products(&self) -> Result<Vec<ProductRow>, PriceStoreError> {
        self.check()?;
        Ok(self
            .products
            .iter()
            .map(|p| ProductRow {
                id: p.id.clone(),
                name: p.name.clone(),
                brand: p.brand.clone(),
                product_name: p.product_name.clone(),
                size: p.size.clone(),
            })
            .collect())
    }

    fn list_all_products(&self) -> Result<Vec<ProductRow>, PriceStoreError> {
        self.list_products()
    }

    fn create_product(
        &self,
        brand: &str,
        product_name: &str,
        name: &str,
        size: &str,
    ) -> Result<ProductInsert, PriceStoreError> {
        self.check()?;
        let exists = self
            .products
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(name))
            || self
                .created_products
                .borrow()
                .iter()
                .any(|(_, _, _, n)| n.eq_ignore_ascii_case(name));
        if exists {
            return Ok(ProductInsert::AlreadyExists);
        }
        self.created_products.borrow_mut().push((
            brand.to_string(),
            product_name.to_string(),
            size.to_string(),
            name.to_string(),
        ));
        Ok(ProductInsert::Created)
    }
}

impl BrandCatalog for FakeStore {
    fn list_brands(&self) -> Result<Vec<BrandRow>, PriceStoreError> {
        self.check()?;
        Ok(self
            .brands
            .iter()
            .map(|b| BrandRow {
                id: b.id.clone(),
                name: b.name.clone(),
            })
            .collect())
    }
}

impl ProviderCatalog for FakeStore {
    fn list_providers(&self) -> Result<Vec<ProviderRow>, PriceStoreError> {
        self.check()?;
        Ok(self
            .providers
            .iter()
            .map(|p| ProviderRow {
                id: p.id.clone(),
                domain: p.domain.clone(),
                name: p.name.clone(),
                enabled: p.enabled,
                default_currency: p.default_currency.clone(),
            })
            .collect())
    }

    fn list_provider_products(&self) -> Result<Vec<ProviderProductRow>, PriceStoreError> {
        self.check()?;
        Ok(self
            .provider_products
            .iter()
            .map(|p| ProviderProductRow {
                id: p.id.clone(),
                provider_id: p.provider_id.clone(),
                name: p.name.clone(),
                product_id: p.product_id.clone(),
                brand_id: p.brand_id.clone(),
            })
            .collect())
    }

    fn update_brand_link(
        &self,
        provider_product_id: &str,
        brand_id: Option<&str>,
    ) -> Result<(), PriceStoreError> {
        self.check()?;
        self.brand_links
            .borrow_mut()
            .insert(provider_product_id.to_string(), brand_id.map(str::to_owned));
        Ok(())
    }

    fn delete_provider_product(&self, provider_product_id: &str) -> Result<(), PriceStoreError> {
        self.check()?;
        self.deleted_provider_products
            .borrow_mut()
            .push(provider_product_id.to_string());
        Ok(())
    }
}

impl MatchStore for FakeStore {
    fn list_above_threshold_candidates(&self) -> Result<Vec<MatchCandidate>, PriceStoreError> {
        self.check()?;
        Ok(self
            .matches()
            .into_iter()
            .filter(|m| m.score >= MIN_SCORE)
            .map(|m| MatchCandidate {
                provider_product_id: m.provider_product_id,
                product_id: m.product_id,
                score: m.score,
            })
            .collect())
    }

    fn list_all_matches(
        &self,
        provider_products: &[ProviderProductRow],
    ) -> Result<Vec<ProviderMatchRow>, PriceStoreError> {
        self.check()?;
        let ids: HashSet<&str> = provider_products.iter().map(|p| p.id.as_str()).collect();
        Ok(self
            .matches()
            .into_iter()
            .filter(|m| ids.contains(m.provider_product_id.as_str()))
            .collect())
    }

    fn create_match(
        &self,
        provider_product_id: &str,
        product_id: &str,
        score: f64,
    ) -> Result<MatchInsert, PriceStoreError> {
        self.check()?;
        let exists =
            self.matches.borrow().iter().any(|m| {
                m.provider_product_id == provider_product_id && m.product_id == product_id
            });
        if exists {
            return Ok(MatchInsert::AlreadyExists);
        }
        self.seed_match(provider_product_id, product_id, score);
        Ok(MatchInsert::Created)
    }

    fn unlink_all(&self, provider_products: &[ProviderProductRow]) -> Result<(), PriceStoreError> {
        self.check()?;
        for pp in provider_products {
            self.product_links.borrow_mut().insert(pp.id.clone(), None);
        }
        Ok(())
    }

    fn link_product(&self, winner: &MatchCandidate) -> Result<(), PriceStoreError> {
        self.check()?;
        self.product_links.borrow_mut().insert(
            winner.provider_product_id.clone(),
            Some(winner.product_id.clone()),
        );
        for m in self.matches.borrow_mut().iter_mut() {
            if m.provider_product_id == winner.provider_product_id
                && m.product_id == winner.product_id
            {
                m.status = "confirmed".to_string();
            }
        }
        Ok(())
    }
}

impl PriceHistory for FakeStore {
    fn latest_price_per_provider_product(&self) -> Result<HashMap<String, f64>, PriceStoreError> {
        self.check()?;
        Ok(self.latest_prices.clone())
    }
}
