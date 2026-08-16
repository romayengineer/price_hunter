#![allow(clippy::cognitive_complexity)]

use std::collections::{HashMap, HashSet};

use ego_tree::NodeId;
use scraper::Html;
use scraper::node::{Element, Node};
use serde::Serialize;

use super::Container;
use super::prices::find_price_divs;

pub(super) fn best_container(
    html: &Html,
    price_divs: &[(NodeId, Vec<super::Price>)],
) -> Option<(NodeId, usize)> {
    ranked_containers(html, price_divs)
        .first()
        .map(|(id, p, _)| (*id, *p))
}

fn ranked_containers(
    html: &Html,
    price_divs: &[(NodeId, Vec<super::Price>)],
) -> Vec<(NodeId, usize, usize)> {
    let price_set: HashSet<NodeId> = price_divs.iter().map(|(id, _)| *id).collect();
    let mut divs: HashMap<NodeId, usize> = HashMap::new();
    let mut prices: HashMap<NodeId, usize> = HashMap::new();
    let nodes: Vec<_> = html.tree.nodes().collect();
    for node in nodes.iter().rev() {
        let Node::Element(el) = node.value() else {
            continue;
        };
        let id = node.id();
        let mut d = usize::from(is_div(el));
        let mut p = usize::from(price_set.contains(&id));
        for child in node.children() {
            if let Node::Element(_) = child.value() {
                d += divs.get(&child.id()).copied().unwrap_or(0);
                p += prices.get(&child.id()).copied().unwrap_or(0);
            }
        }
        divs.insert(id, d);
        prices.insert(id, p);
    }
    let mut candidates: Vec<(NodeId, usize, usize)> = nodes
        .iter()
        .filter_map(|node| {
            let Node::Element(el) = node.value() else {
                return None;
            };
            if !is_div(el) {
                return None;
            }
            let p = *prices.get(&node.id())?;
            let d = *divs.get(&node.id())?;
            (p >= 2).then_some((node.id(), p, d))
        })
        .collect();
    let max_p = candidates.iter().map(|(_, p, _)| *p).max().unwrap_or(0);
    candidates.retain(|(_, p, _)| *p >= max_p / 2);
    candidates.sort_by(|a, b| {
        let da = a.1 as f64 / a.2 as f64;
        let db = b.1 as f64 / b.2 as f64;
        db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContainerCandidate {
    pub classes: Vec<String>,
    pub id: Option<String>,
    pub price_count: usize,
    pub div_count: usize,
    pub density: f64,
    pub selected: bool,
}

/// Ranks every candidate product container in the page, newest first. The
/// `selected` flag marks the one `detect_grid` would pick. Useful for
/// diagnosing why a grid on a whole page (with widgets, carousels, etc.)
/// resolves to the wrong container.
pub fn diagnose_containers(source: &str) -> Vec<ContainerCandidate> {
    let html = Html::parse_document(source);
    let price_divs = find_price_divs(&html);
    let picked = best_container(&html, &price_divs).map(|(id, _)| id);
    ranked_containers(&html, &price_divs)
        .into_iter()
        .map(|(id, p, d)| ContainerCandidate {
            classes: html
                .tree
                .get(id)
                .and_then(|n| match n.value() {
                    Node::Element(el) => Some(el.classes().map(|c| c.to_string()).collect()),
                    _ => None,
                })
                .unwrap_or_default(),
            id: html.tree.get(id).and_then(|n| match n.value() {
                Node::Element(el) => el.attr("id").map(|s| s.to_string()),
                _ => None,
            }),
            price_count: p,
            div_count: d,
            density: p as f64 / d as f64,
            selected: picked == Some(id),
        })
        .collect()
}

pub(super) fn build_container(html: &Html, id: NodeId, child_count: usize) -> Container {
    let mut classes = Vec::new();
    let mut element_id = None;
    if let Some(Node::Element(el)) = html.tree.get(id).map(|n| n.value()) {
        classes = el.classes().map(|c| c.to_string()).collect();
        element_id = el.attr("id").map(|s| s.to_string());
    }
    Container {
        classes,
        id: element_id,
        child_count,
    }
}

fn is_div(el: &Element) -> bool {
    el.name() == "div"
}
