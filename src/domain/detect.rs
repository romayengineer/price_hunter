#![allow(clippy::cognitive_complexity)]

use scraper::Html;
use serde::Serialize;

mod container;
mod extract;
mod prices;

#[cfg(test)]
mod tests;

pub use container::{ContainerCandidate, diagnose_containers};
use container::{best_container, build_container};
use extract::extract_products;
use prices::find_price_divs;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Price {
    pub value: f64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Product {
    pub name: String,
    pub price_text: String,
    pub price: f64,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub images: Vec<String>,
    #[serde(default)]
    pub currency: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Container {
    pub classes: Vec<String>,
    pub id: Option<String>,
    pub child_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Detection {
    pub container: Container,
    pub products: Vec<Product>,
}

pub fn detect_grid(source: &str) -> Option<Detection> {
    let html = Html::parse_document(source);
    let price_divs = find_price_divs(&html);
    let (container_id, child_count) = best_container(&html, &price_divs)?;
    Some(Detection {
        container: build_container(&html, container_id, child_count),
        products: extract_products(&html, container_id, &price_divs),
    })
}
