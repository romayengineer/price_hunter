#![allow(clippy::cognitive_complexity)]

use std::collections::{HashMap, HashSet};

use ego_tree::{NodeId, NodeRef};
use scraper::Html;
use scraper::node::{Element, Node};

use super::Price;
use super::extract::{find_structured_name, largest_text_block};
use price_hunter_domain::text as domain_text;

pub(super) fn find_price_divs(html: &Html) -> Vec<(NodeId, Vec<Price>)> {
    let own = own_texts(html);
    let leaf = if has_product_price_spans(html) {
        let candidates = price_divs_from_product_price_spans(html);
        if candidates.len() >= 2 {
            candidates
        } else {
            price_divs_from_own_text(html, &own)
        }
    } else {
        price_divs_from_own_text(html, &own)
    };
    let merged = merge_price_groups(html, &own, leaf);
    drop_ancestor_price_divs(html, merged)
}

fn drop_ancestor_price_divs(
    html: &Html,
    price_divs: Vec<(NodeId, Vec<Price>)>,
) -> Vec<(NodeId, Vec<Price>)> {
    let ids: Vec<NodeId> = price_divs.iter().map(|(id, _)| *id).collect();
    price_divs
        .into_iter()
        .filter(|(id, _)| {
            !ids.iter()
                .any(|other| *other != *id && is_ancestor_of(html, *id, *other))
        })
        .collect()
}

fn is_ancestor_of(html: &Html, ancestor: NodeId, descendant: NodeId) -> bool {
    let Some(node) = html.tree.get(descendant) else {
        return false;
    };
    node.ancestors().any(|a| a.id() == ancestor)
}

fn price_divs_from_own_text(
    html: &Html,
    own: &HashMap<NodeId, String>,
) -> Vec<(NodeId, Vec<Price>)> {
    let mut out = Vec::new();
    for node in html.tree.nodes() {
        let Node::Element(el) = node.value() else {
            continue;
        };
        if !is_div(el) {
            continue;
        }
        let Some(text) = own.get(&node.id()) else {
            continue;
        };
        if let Some(prices) = classify_div(text) {
            out.push((node.id(), prices));
        }
    }
    out
}

fn has_product_price_spans(html: &Html) -> bool {
    html.tree.nodes().any(|node| {
        let Node::Element(el) = node.value() else {
            return false;
        };
        el.name() == "span" && el.classes().any(|c| c.contains("product-price"))
    })
}

fn price_divs_from_product_price_spans(html: &Html) -> Vec<(NodeId, Vec<Price>)> {
    let mut div_text: HashMap<NodeId, String> = HashMap::new();
    for node in html.tree.nodes() {
        let Node::Element(el) = node.value() else {
            continue;
        };
        if el.name() != "span" || !el.classes().any(|c| c.contains("product-price")) {
            continue;
        }
        let own: String = node
            .children()
            .filter_map(|c| match c.value() {
                Node::Text(t) => Some(&*t.text),
                _ => None,
            })
            .collect();
        if own.is_empty() {
            continue;
        }
        let Some(div_id) = nearest_div_ancestor(&node) else {
            continue;
        };
        div_text.entry(div_id).or_default().push_str(&own);
    }
    let mut out = Vec::new();
    for node in html.tree.nodes() {
        let Node::Element(el) = node.value() else {
            continue;
        };
        if !is_div(el) {
            continue;
        }
        let Some(text) = div_text.get(&node.id()) else {
            continue;
        };
        if let Some(prices) = classify_div(text) {
            out.push((node.id(), prices));
        }
    }
    out
}

fn nearest_div_ancestor(node: &NodeRef<'_, Node>) -> Option<NodeId> {
    node.ancestors().find_map(|a| match a.value() {
        Node::Element(el) if is_div(el) => Some(a.id()),
        _ => None,
    })
}

fn merge_price_groups(
    html: &Html,
    own: &HashMap<NodeId, String>,
    leaf: Vec<(NodeId, Vec<Price>)>,
) -> Vec<(NodeId, Vec<Price>)> {
    let leaf_ids: HashSet<NodeId> = leaf.iter().map(|(id, _)| *id).collect();
    let mut merged_into: HashMap<NodeId, NodeId> = HashMap::new();
    let nodes: Vec<_> = html.tree.nodes().collect();
    for node in nodes.iter() {
        let Node::Element(el) = node.value() else {
            continue;
        };
        if !is_div(el) {
            continue;
        }
        let id = node.id();
        let price_children: Vec<NodeId> = node
            .children()
            .filter(|c| leaf_ids.contains(&c.id()))
            .map(|c| c.id())
            .collect();
        if price_children.len() < 2 {
            continue;
        }
        if !own.get(&id).map(|t| t.trim().is_empty()).unwrap_or(true) {
            continue;
        }
        if price_children
            .iter()
            .any(|cid| price_div_is_named(html, *cid))
        {
            continue;
        }
        for cid in &price_children {
            merged_into.insert(*cid, id);
        }
    }
    let mut result: Vec<(NodeId, Vec<Price>)> = Vec::new();
    for (cid, prices) in leaf {
        let pid = merged_into.get(&cid).copied().unwrap_or(cid);
        if let Some(entry) = result.iter_mut().find(|(eid, _)| *eid == pid) {
            entry.1 = prices;
        } else {
            result.push((pid, prices));
        }
    }
    result
}

fn price_div_is_named(html: &Html, id: NodeId) -> bool {
    let Some(node) = html.tree.get(id) else {
        return false;
    };
    if find_structured_name(&node).is_some() {
        return true;
    }
    largest_text_block(&node)
        .map(|b| b.chars().count() >= 6)
        .unwrap_or(false)
}

fn own_texts(html: &Html) -> HashMap<NodeId, String> {
    let mut map: HashMap<NodeId, String> = HashMap::new();
    for node in html.tree.nodes() {
        let Node::Text(text) = node.value() else {
            continue;
        };
        for anc in node.ancestors() {
            if let Node::Element(el) = anc.value()
                && is_div(el)
            {
                map.entry(anc.id()).or_default().push_str(&text.text);
                break;
            }
        }
    }
    map
}

fn is_div(el: &Element) -> bool {
    el.name() == "div"
}

pub(super) fn classify_div(text: &str) -> Option<Vec<Price>> {
    domain_text::classify_text(text).map(|prices| {
        prices
            .into_iter()
            .map(|p| Price {
                value: p.value,
                text: p.text,
            })
            .collect()
    })
}

pub(super) fn contains_confident_price(text: &str) -> bool {
    domain_text::contains_confident_price(text)
}

/// Test/compat shims: the canonical implementations live in
/// `price_hunter_domain::text`; kept here so `super::prices::…` paths keep
/// working.
#[allow(dead_code)]
pub(super) fn number_tokens(text: &str) -> Vec<String> {
    domain_text::number_tokens(text)
}

/// Test/compat shim over [`domain_text::parse_price`][price_hunter_domain::text::parse_price].
#[allow(dead_code)]
pub(super) fn parse_price(token: &str) -> Option<f64> {
    domain_text::parse_price(token)
}
