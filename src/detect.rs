#![allow(clippy::cognitive_complexity)]

use std::collections::{HashMap, HashSet};

use ego_tree::{NodeId, NodeRef};
use scraper::Html;
use scraper::node::{Element, Node};
use serde::Serialize;

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

fn find_price_divs(html: &Html) -> Vec<(NodeId, Vec<Price>)> {
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
            !ids.iter().any(|other| {
                *other != *id && is_ancestor_of(html, *id, *other)
            })
        })
        .collect()
}

fn is_ancestor_of(html: &Html, ancestor: NodeId, descendant: NodeId) -> bool {
    let Some(node) = html.tree.get(descendant) else {
        return false;
    };
    node.ancestors().any(|a| a.id() == ancestor)
}

fn price_divs_from_own_text(html: &Html, own: &HashMap<NodeId, String>) -> Vec<(NodeId, Vec<Price>)> {
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

fn classify_div(text: &str) -> Option<Vec<Price>> {
    let tokens = number_tokens(text);
    let confident: Vec<Price> = tokens
        .iter()
        .filter(|t| has_separator(t))
        .filter_map(|t| {
            parse_price(t).map(|value| Price {
                value,
                text: t.clone(),
            })
        })
        .collect();
    if !confident.is_empty() {
        return Some(confident);
    }
    let bare: Vec<&str> = tokens
        .iter()
        .filter(|t| !has_separator(t) && (2..=7).contains(&t.len()))
        .map(|s| s.as_str())
        .collect();
    if bare.len() == 1 && !text_has_content_other_than(text, bare[0]) {
        let t = bare[0];
        return Some(vec![Price {
            value: t.parse().ok()?,
            text: t.to_string(),
        }]);
    }
    None
}

fn text_has_content_other_than(text: &str, token: &str) -> bool {
    text.replacen(token, "", 1)
        .chars()
        .any(|c| c.is_alphanumeric() || matches!(c, '-' | '%'))
}

fn contains_confident_price(text: &str) -> bool {
    number_tokens(text).iter().any(|t| has_separator(t))
}

fn has_separator(token: &str) -> bool {
    token
        .chars()
        .any(|c| matches!(c, ' ' | '\u{a0}' | '.' | ',' | '\''))
}

fn number_tokens(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len() {
            let c = chars[j];
            if c.is_ascii_digit() {
                j += 1;
            } else if matches!(c, '.' | ',' | '\'') && j + 1 < chars.len() && chars[j + 1].is_ascii_digit() {
                j += 2;
            } else if matches!(c, ' ' | '\u{a0}') && is_thousands_group(&chars, j) {
                j += 1;
            } else {
                break;
            }
        }
        out.push(chars[i..j].iter().collect());
        i = j;
    }
    out
}

fn is_thousands_group(chars: &[char], j: usize) -> bool {
    let mut k = j + 1;
    while k < chars.len() && chars[k].is_ascii_digit() {
        k += 1;
    }
    k - (j + 1) == 3
}

fn parse_price(token: &str) -> Option<f64> {
    let (int_part, frac) = split_decimal(token);
    let int_digits: String = int_part.chars().filter(|c| c.is_ascii_digit()).collect();
    if int_digits.is_empty() {
        return None;
    }
    let mut num = int_digits;
    if let Some(frac) = frac {
        num.push('.');
        num.push_str(&frac);
    }
    num.parse().ok()
}

fn split_decimal(token: &str) -> (String, Option<String>) {
    let dot = token.rfind('.');
    let comma = token.rfind(',');
    let sep = match (dot, comma) {
        (Some(d), Some(c)) if d > c => Some(d),
        (Some(_), Some(c)) => Some(c),
        (Some(d), None) => {
            if digits_after(token, d) <= 2 {
                Some(d)
            } else {
                None
            }
        }
        (None, Some(c)) => {
            if digits_after(token, c) <= 2 {
                Some(c)
            } else {
                None
            }
        }
        (None, None) => None,
    };
    match sep {
        Some(i) => {
            let int_part = &token[..i];
            let frac: String = token[i + 1..]
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect();
            (int_part.to_string(), Some(frac))
        }
        None => (token.to_string(), None),
    }
}

fn digits_after(s: &str, from: usize) -> usize {
    s[from + 1..]
        .chars()
        .filter(|c| c.is_ascii_digit())
        .count()
}

fn best_container(html: &Html, price_divs: &[(NodeId, Vec<Price>)]) -> Option<(NodeId, usize)> {
    ranked_containers(html, price_divs)
        .first()
        .map(|(id, p, _)| (*id, *p))
}

fn ranked_containers(
    html: &Html,
    price_divs: &[(NodeId, Vec<Price>)],
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

fn build_container(html: &Html, id: NodeId, child_count: usize) -> Container {
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

fn extract_products(
    html: &Html,
    container_id: NodeId,
    price_divs: &[(NodeId, Vec<Price>)],
) -> Vec<Product> {
    let mut cards: Vec<(NodeId, NodeId, Vec<Price>)> = Vec::new();
    for (id, prices) in price_divs {
        if !is_descendant_of(html, *id, container_id) {
            continue;
        }
        let key = card_of(html, *id, container_id);
        match cards.iter_mut().find(|(k, _, _)| *k == key) {
            Some((_, best_id, best_prices)) => {
                if prices.len() > best_prices.len() {
                    *best_id = *id;
                    *best_prices = prices.clone();
                }
            }
            None => cards.push((key, *id, prices.clone())),
        }
    }
    cards
        .into_iter()
        .map(|(_, price_div_id, prices)| {
            let price = current_price_of(html, price_div_id)
                .or_else(|| prices.last().cloned())
                .expect("price div has a price");
            let card_id = card_of(html, price_div_id, container_id);
            let url = product_link(html, card_id);
            Product {
                name: enrich_name_with_size(
                    html,
                    card_id,
                    guess_name(html, price_div_id, container_id),
                    url.as_deref().unwrap_or(""),
                ),
                price_text: price.text.clone(),
                price: price.value,
                url,
                images: card_images(html, card_id),
                currency: detect_currency(html, card_id),
            }
        })
        .collect()
}

fn current_price_of(html: &Html, id: NodeId) -> Option<Price> {
    let node = html.tree.get(id)?;
    for n in node.descendants() {
        let Node::Element(el) = n.value() else {
            continue;
        };
        let is_current_price = el.attr("itemprop") == Some("price")
            || el.attr("data-price-type") == Some("finalPrice");
        if !is_current_price {
            continue;
        }
        let text: String = n
            .descendants()
            .filter_map(|x| match x.value() {
                Node::Text(t) => Some(&*t.text),
                _ => None,
            })
            .collect();
        if let Some(prices) = classify_div(&text) {
            return prices.last().cloned();
        }
    }
    None
}

fn card_of(html: &Html, id: NodeId, container_id: NodeId) -> NodeId {
    let Some(mut node) = html.tree.get(id) else {
        return id;
    };
    let mut child = id;
    loop {
        let Some(parent) = node.parent() else {
            return child;
        };
        if parent.id() == container_id {
            if is_card_list_wrapper(&node) {
                return child;
            }
            return node.id();
        }
        child = node.id();
        node = parent;
    }
}

fn is_card_list_wrapper(node: &NodeRef<'_, Node>) -> bool {
    match node.value() {
        Node::Element(el) => {
            matches!(el.name(), "ul" | "ol" | "tbody" | "table") || el.attr("role") == Some("list")
        }
        _ => false,
    }
}

fn is_descendant_of(html: &Html, id: NodeId, container_id: NodeId) -> bool {
    let Some(node) = html.tree.get(id) else {
        return false;
    };
    node.ancestors().any(|a| a.id() == container_id)
}

fn guess_name(html: &Html, id: NodeId, container_id: NodeId) -> String {
    let Some(mut node) = html.tree.get(id) else {
        return String::new();
    };
    let mut best_block = String::new();
    loop {
        if let Some(name) = find_structured_name(&node) {
            return name;
        }
        if let Some(block) = largest_text_block(&node)
            && block.chars().count() > best_block.chars().count()
        {
            best_block = block;
        }
        let Some(parent) = node.parent() else {
            return best_block;
        };
        if parent.id() == container_id {
            return best_block;
        }
        node = parent;
    }
}

/// Recognized product-size units, case-insensitive.
fn is_size_unit(unit: &str) -> bool {
    matches!(unit.to_ascii_lowercase().as_str(), "ml" | "g" | "gr" | "l" | "lt")
}

/// Returns the first `N unit` substring in `text` (e.g. `100 ml`, `100ml`,
/// `100 Ml`, `X50ML`, `132 g`), preserving its original spacing/case. Used to
/// detect whether a name already carries its size and to lift the size out of
/// SKU selectors and product URLs.
fn find_size_in_text(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let num_start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        let mut j = i;
        if j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        let unit_start = j;
        while j < chars.len() && chars[j].is_ascii_alphabetic() {
            j += 1;
        }
        if unit_start < j && is_size_unit(&chars[unit_start..j].iter().collect::<String>()) {
            let matched: String = chars[num_start..j].iter().collect();
            return Some(collapse_whitespace(&matched));
        }
        i = j;
    }
    None
}

/// Whether `name` already carries a size (number + unit).
fn has_size(name: &str) -> bool {
    find_size_in_text(name).is_some()
}

/// Whether `name` ends in a bare number (e.g. `edp 50`) with no unit.
fn has_trailing_bare_number(name: &str) -> bool {
    name.split_whitespace()
        .next_back()
        .is_some_and(|token| !token.is_empty() && token.chars().all(|c| c.is_ascii_digit()))
}

/// Lifts the product size out of a VTEX-style SKU selector inside the card.
/// Prefers the option marked `--selected`, falling back to the first one that
/// carries a size.
fn size_from_sku_selector(html: &Html, card_id: NodeId) -> Option<String> {
    let node = html.tree.get(card_id)?;
    let mut first: Option<String> = None;
    for n in node.descendants() {
        let Node::Element(el) = n.value() else {
            continue;
        };
        if !el.classes().any(|c| c.contains("skuSelectorItem")) {
            continue;
        }
        let text: String = n
            .descendants()
            .filter_map(|x| match x.value() {
                Node::Text(t) => Some(&*t.text),
                _ => None,
            })
            .collect();
        let text = collapse_whitespace(&text);
        let Some(size) = find_size_in_text(&text) else {
            continue;
        };
        if el.classes().any(|c| c.contains("selected")) {
            return Some(size);
        }
        if first.is_none() {
            first = Some(size);
        }
    }
    first
}

/// Lifts the product size out of a product URL slug (e.g. `...-100ml-...`).
fn size_from_url(url: &str) -> Option<String> {
    find_size_in_text(url)
}

/// Ensures the extracted product name carries its size. Names that already
/// include a size are returned unchanged. Otherwise the size is taken from the
/// card's SKU selector (selected option first), then from the product URL, and
/// finally by appending `ml` to a trailing bare number (e.g. `edp 50`).
fn enrich_name_with_size(html: &Html, card_id: NodeId, name: String, url: &str) -> String {
    if has_size(&name) {
        return name;
    }
    if let Some(size) = size_from_sku_selector(html, card_id) {
        return format!("{name} {size}");
    }
    if let Some(size) = size_from_url(url) {
        return format!("{name} {size}");
    }
    if has_trailing_bare_number(&name) {
        return format!("{name} ml");
    }
    name
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_structured_name(node: &NodeRef<'_, Node>) -> Option<String> {
    for n in node.descendants() {
        let Node::Element(el) = n.value() else {
            continue;
        };
        if el.name() != "a" {
            continue;
        }
        let is_product_name = el.attr("data-role") == Some("product-item-name")
            || el.classes().any(|c| c.contains("product-item-name"));
        if !is_product_name {
            continue;
        }
        for attr in ["title", "aria-label"] {
            if let Some(t) = el.attr(attr) {
                let t = t.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
        let text: String = n
            .descendants()
            .filter_map(|x| match x.value() {
                Node::Text(t) => Some(&*t.text),
                _ => None,
            })
            .collect();
        let text = collapse_whitespace(&text);
        if !text.is_empty() && !contains_confident_price(&text) {
            return Some(text);
        }
    }
    for n in node.descendants() {
        match n.value() {
            Node::Element(el) if el.name() == "a" => {
                for attr in ["title", "aria-label"] {
                    if let Some(t) = el.attr(attr) {
                        let t = t.trim();
                        if !t.is_empty() {
                            return Some(t.to_string());
                        }
                    }
                }
                let text: String = n
                    .descendants()
                    .filter_map(|x| match x.value() {
                        Node::Text(t) => Some(&*t.text),
                        _ => None,
                    })
                    .collect();
                let text = collapse_whitespace(&text);
                if !text.is_empty() && !contains_confident_price(&text) {
                    return Some(text);
                }
            }
            Node::Element(el) if el.name() == "img" => {
                if let Some(a) = el.attr("alt") {
                    let a = a.trim();
                    if !a.is_empty() {
                        return Some(a.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn product_link(html: &Html, card_id: NodeId) -> Option<String> {
    let node = html.tree.get(card_id)?;
    let mut fallback = None;
    let mut titled = None;
    for n in node.descendants() {
        let Node::Element(el) = n.value() else {
            continue;
        };
        if el.name() != "a" {
            continue;
        }
        let Some(href) = el.attr("href").map(str::trim) else {
            continue;
        };
        if href.is_empty() || is_placeholder_href(href) {
            continue;
        }
        let is_structured = el.attr("data-role") == Some("product-item-name")
            || el.classes().any(|c| c.contains("product-item-name"));
        if is_structured {
            return Some(href.to_string());
        }
        if fallback.is_none() {
            fallback = Some(href.to_string());
        }
        if titled.is_none() && anchor_has_text(html, n.id()) {
            titled = Some(href.to_string());
        }
    }
    titled.or(fallback)
}

/// True for links that don't point anywhere useful for a product page:
/// `#` anchors, `javascript:` stubs, `mailto:`/`tel:`, and bare fragment
/// links (e.g. `https://site/category#`). Those are usually icon/button
/// links that appear before the real product link in the card.
fn is_placeholder_href(href: &str) -> bool {
    let href = href.trim();
    if href.is_empty() || href == "#" {
        return true;
    }
    if ["javascript:", "mailto:", "tel:", "data:"]
        .iter()
        .any(|prefix| href.starts_with(prefix))
    {
        return true;
    }
    href.ends_with('#')
}

/// Whether the anchor carries any text of its own (a product-title link) as
/// opposed to being an icon-only link (image + no text).
fn anchor_has_text(html: &Html, id: NodeId) -> bool {
    let Some(node) = html.tree.get(id) else {
        return false;
    };
    let mut text = String::new();
    for child in node.descendants() {
        if let Node::Text(t) = child.value() {
            text.push_str(&t.text);
        }
    }
    !collapse_whitespace(&text).is_empty()
}

fn card_images(html: &Html, card_id: NodeId) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let Some(node) = html.tree.get(card_id) else {
        return out;
    };
    for n in node.descendants() {
        let Node::Element(el) = n.value() else {
            continue;
        };
        if el.name() != "img" {
            continue;
        }
        let Some(src) = el.attr("src").map(str::trim) else {
            continue;
        };
        if src.is_empty() {
            continue;
        }
        if seen.insert(src.to_string()) {
            out.push(src.to_string());
        }
    }
    out
}

fn detect_currency(html: &Html, card_id: NodeId) -> Option<String> {
    let node = html.tree.get(card_id)?;
    let text: String = node
        .descendants()
        .filter_map(|x| match x.value() {
            Node::Text(t) => Some(&*t.text),
            _ => None,
        })
        .collect();
    let text = text.to_uppercase();
    for (needle, code) in [
        ("US$", "USD"),
        ("U$S", "USD"),
        ("USD", "USD"),
        ("AR$", "ARS"),
        ("ARS", "ARS"),
        ("\u{20ac}", "EUR"),
        ("EUR", "EUR"),
        ("\u{a3}", "GBP"),
        ("GBP", "GBP"),
        ("R$", "BRL"),
        ("BRL", "BRL"),
    ] {
        if text.contains(needle) {
            return Some(code.to_string());
        }
    }
    None
}

fn largest_text_block(node: &NodeRef<'_, Node>) -> Option<String> {
    let mut best_block = String::new();
    for n in node.descendants() {
        if let Node::Text(t) = n.value() {
            let s = collapse_whitespace(&t.text);
            if !s.is_empty()
                && !contains_confident_price(&s)
                && s.chars().any(|c| c.is_alphabetic())
                && s.chars().count() > best_block.chars().count()
            {
                best_block = s;
            }
        }
    }
    if best_block.is_empty() {
        None
    } else {
        Some(best_block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid_page() -> String {
        r#"
        <html><body>
          <div id="wrapper">
            <h1>Shop</h1>
            <div class="product-grid">
              <div class="card"><a href="/p1">Widget A</a><span class="price">$12.99</span></div>
              <div class="card"><a href="/p2">Widget B</a><span class="price">19,95</span></div>
              <div class="card"><a href="/p3">Widget C</a><span class="price">1.234,56</span></div>
              <div class="card"><a href="/p4">Widget D</a><span class="price">1,299.00</span></div>
            </div>
            <div id="footer">Contact us</div>
          </div>
        </body></html>
        "#
        .to_string()
    }

    #[test]
    fn detects_grid_with_mixed_formats() {
        let detection = detect_grid(&grid_page()).expect("grid should be detected");
        assert_eq!(detection.container.classes, vec!["product-grid"]);
        assert_eq!(detection.products.len(), 4);
        assert_eq!(detection.products[0].name, "Widget A");
        assert_eq!(detection.products[0].price, 12.99);
        assert_eq!(detection.products[1].price, 19.95);
        assert_eq!(detection.products[2].price, 1234.56);
        assert_eq!(detection.products[3].price, 1299.0);
    }

    #[test]
    fn no_grid_without_prices() {
        let html = "<html><body><div><p>No prices here.</p></div></body></html>";
        assert!(detect_grid(html).is_none());
    }

    #[test]
    fn single_price_div_is_not_a_grid() {
        let html = "<html><body><div class=\"card\"><span>12,99</span></div></body></html>";
        assert!(detect_grid(html).is_none());
    }

    #[test]
    fn nested_wrapper_divs_inside_cards() {
        let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card"><div class="inner"><span class="price">10,00</span></div></div>
            <div class="card"><div class="inner"><span class="price">20,00</span></div></div>
            <div class="card"><div class="inner"><span class="price">30,00</span></div></div>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.container.classes, vec!["product-grid"]);
        assert_eq!(detection.products.len(), 3);
    }

    #[test]
    fn picks_larger_of_two_grids() {
        let html = r#"
        <html><body>
          <div class="small-grid">
            <div class="card"><a>Alpha</a><span>1,00</span></div>
            <div class="card"><a>Beta</a><span>2,00</span></div>
          </div>
          <div class="big-grid">
            <div class="card"><a>One</a><span>1,00</span></div>
            <div class="card"><a>Two</a><span>2,00</span></div>
            <div class="card"><a>Three</a><span>3,00</span></div>
            <div class="card"><a>Four</a><span>4,00</span></div>
            <div class="card"><a>Five</a><span>5,00</span></div>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.container.classes, vec!["big-grid"]);
        assert_eq!(detection.products.len(), 5);
    }

    #[test]
    fn bare_integer_price_is_detected() {
        let html = r#"
        <html><body>
          <div class="grid">
            <div class="card"><div class="price"><span>499</span></div></div>
            <div class="card"><div class="price"><span>899</span></div></div>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.products.len(), 2);
        assert_eq!(detection.products[0].price, 499.0);
    }

    #[test]
    fn thousands_only_price() {
        let html = r#"
        <html><body>
          <div class="grid">
            <div class="card"><div class="price"><span>1,234</span></div></div>
            <div class="card"><div class="price"><span>5.678</span></div></div>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.products[0].price, 1234.0);
        assert_eq!(detection.products[1].price, 5678.0);
    }

    #[test]
    fn parse_price_woocommerce_thousands() {
        assert_eq!(parse_price("8.190"), Some(8190.0));
        assert_eq!(parse_price("12.990"), Some(12990.0));
        assert_eq!(parse_price("3.450"), Some(3450.0));
        assert_eq!(parse_price("8,190"), Some(8190.0));
    }

    #[test]
    fn detects_woocommerce_price_grid() {
        let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card"><a href="/a">Alpha</a><bdi><span class="woocommerce-Price-currencySymbol">$</span>8.190</bdi></div>
            <div class="card"><a href="/b">Beta</a><bdi><span class="woocommerce-Price-currencySymbol">$</span>12.990</bdi></div>
            <div class="card"><a href="/c">Gamma</a><bdi><span class="woocommerce-Price-currencySymbol">$</span>3.450</bdi></div>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.container.classes, vec!["product-grid"]);
        assert_eq!(detection.products.len(), 3);
        assert_eq!(detection.products[0].name, "Alpha");
        assert_eq!(detection.products[1].name, "Beta");
        assert_eq!(detection.products[2].name, "Gamma");
        assert_eq!(detection.products[0].price, 8190.0);
        assert_eq!(detection.products[1].price, 12990.0);
        assert_eq!(detection.products[2].price, 3450.0);
        assert_eq!(detection.products[0].price_text, "8.190");
        assert_eq!(detection.products[1].price_text, "12.990");
        assert_eq!(detection.products[2].price_text, "3.450");
    }

    #[test]
    fn currency_symbol_alone_is_not_a_price() {
        assert!(number_tokens("$").is_empty());
        let price = classify_div("$8.190").expect("price should be found");
        assert_eq!(price.len(), 1);
        assert_eq!(price[0].value, 8190.0);
        assert_eq!(price[0].text, "8.190");
    }

    #[test]
    fn prestashop_prefers_itemprop_price() {
        let html = r#"
        <html><body>
          <div class="products row">
            <article class="product-miniature">
              <div class="product-description">
                <h2 class="product-title"><a href="/a">Light Blue Homme EDP 50</a></h2>
                <div class="product-price-and-shipping">
                  <span itemprop="price" class="price cod"><span>$242.100</span></span>
                  <span class="regular-price">$269.000</span>
                  <span class="discount-percentage discount-product">-10%</span>
                </div>
              </div>
            </article>
            <article class="product-miniature">
              <div class="product-description">
                <h2 class="product-title"><a href="/b">Bottled Beyond EDT 50</a></h2>
                <div class="product-price-and-shipping">
                  <span itemprop="price" class="price cod"><span>$219.000</span></span>
                  <span class="regular-price">$249.000</span>
                  <span class="discount-amount discount-product">-$30.000</span>
                </div>
              </div>
            </article>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.container.classes, vec!["products", "row"]);
        assert_eq!(detection.products.len(), 2);
        assert_eq!(detection.products[0].name, "Light Blue Homme EDP 50 ml");
        assert_eq!(detection.products[0].price, 242100.0);
        assert_eq!(detection.products[0].price_text, "242.100");
        assert_eq!(detection.products[1].name, "Bottled Beyond EDT 50 ml");
        assert_eq!(detection.products[1].price, 219000.0);
        assert_eq!(detection.products[1].price_text, "219.000");
    }

    #[test]
    fn magento_ul_list_splits_cards_and_prefers_final_price() {
        let html = r#"
        <html><body>
          <div class="products wrapper mode-grid products-grid">
            <ul role="list">
              <li>
                <form class="product_addtocart_form">
                  <strong class="product brand"><a class="product-item-link" href="/brands/x">RABANNE</a></strong>
                  <a class="product-item-link" data-role="product-item-name" href="/a">FAME COUTURE EDP 80ML</a>
                  <div class="price-box price-final_price">
                    <span class="price-wrapper price-including-tax" data-price-type="finalPrice"><span class="price">$&nbsp;264.000</span></span>
                    <span class="price-wrapper price-excluding-tax" data-price-type="basePrice"><span class="price">$&nbsp;218.182</span></span>
                  </div>
                  <div class="product-installments"><span class="amount">$&nbsp;22.000</span></div>
                </form>
              </li>
              <li>
                <form class="product_addtocart_form">
                  <strong class="product brand"><a class="product-item-link" href="/brands/y">CAROLINA HERRERA</a></strong>
                  <a class="product-item-link" data-role="product-item-name" href="/b">212 SEXY MEN EDT 100ML</a>
                  <div class="price-box price-final_price">
                    <span class="old-price"><span class="price-wrapper price-including-tax" data-price-type="oldPrice"><span class="price">$&nbsp;225.000</span></span></span>
                    <span class="normal-price"><span class="price-wrapper price-including-tax" data-price-type="finalPrice"><span class="price">$&nbsp;165.000</span></span></span>
                    <span class="price-wrapper price-excluding-tax" data-price-type="basePrice"><span class="price">$&nbsp;165.000</span></span>
                  </div>
                </form>
              </li>
            </ul>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.container.classes, vec!["mode-grid", "products", "products-grid", "wrapper"]);
        assert_eq!(detection.products.len(), 2);
        assert_eq!(detection.products[0].name, "FAME COUTURE EDP 80ML");
        assert_eq!(detection.products[0].price, 264000.0);
        assert_eq!(detection.products[0].price_text, "264.000");
        assert_eq!(detection.products[1].name, "212 SEXY MEN EDT 100ML");
        assert_eq!(detection.products[1].price, 165000.0);
        assert_eq!(detection.products[1].price_text, "165.000");
    }

    #[test]
    fn two_prices_card_picks_current_price() {
        let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card"><a href="/x">Kappa</a><bdi><span class="woocommerce-Price-currencySymbol">€</span>12,99</bdi></div>
            <div class="card"><a href="/y">Lambda</a><bdi><span class="woocommerce-Price-currencySymbol">€</span>24,50</bdi></div>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.products.len(), 2);
        assert_eq!(detection.products[0].price, 12.99);
        assert_eq!(detection.products[0].price_text, "12,99");
        assert_eq!(detection.products[1].price, 24.5);
    }

    #[test]
    fn captures_product_url_images_and_currency() {
        let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card">
              <a data-role="product-item-name" href="/perfumes/alpha">Alpha EDP 50</a>
              <img src="/img/alpha-1.jpg" alt="Alpha EDP 50">
              <img src="/img/alpha-1.jpg" alt="Alpha EDP 50">
              <img src="/img/alpha-2.jpg" alt="Alpha EDP 50">
              <span class="price">$&nbsp;242.100</span>
            </div>
            <div class="card">
              <a data-role="product-item-name" href="/perfumes/beta">Beta EDP 50</a>
              <img src="/img/beta.jpg" alt="Beta EDP 50">
              <span class="price">AR$&nbsp;99.900</span>
            </div>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.products.len(), 2);
        assert_eq!(detection.products[0].url.as_deref(), Some("/perfumes/alpha"));
        assert_eq!(
            detection.products[0].images,
            vec!["/img/alpha-1.jpg".to_string(), "/img/alpha-2.jpg".to_string()]
        );
        assert_eq!(detection.products[1].url.as_deref(), Some("/perfumes/beta"));
        assert_eq!(detection.products[1].images, vec!["/img/beta.jpg".to_string()]);
        assert_eq!(detection.products[1].currency.as_deref(), Some("ARS"));
    }

    #[test]
    fn bare_dollar_does_not_force_a_currency() {
        let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card"><a href="/a">Alpha</a><span class="price">$ 12.990</span></div>
            <div class="card"><a href="/b">Beta</a><span class="price">$ 8.190</span></div>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert!(detection.products.iter().all(|p| p.currency.is_none()));
    }

    #[test]
    fn product_link_skips_placeholder_anchors() {
        // compreahora-style card: icon/button anchors come first in the card
        // and must not be picked as the product URL.
        let html = r##"
        <html><body>
          <div class="product-grid">
            <div class="card">
              <a href="https://www.compreahora.com.ar/categoria/perfumeria#" class="shopping-list-icon"><img alt="Axe Gold"></a>
              <a href="javascript:void(0)"><img alt="Axe Gold"></a>
              <a href="#"><span class="icon"></span></a>
              <h3><a href="/producto/desodorante-axe-gold-vainilla-en-aerosol-150-ml">Desodorante Axe Gold vainilla en aerosol 150 ml</a></h3>
              <span class="price">$ 3.744,05</span>
            </div>
            <div class="card">
              <a href="https://www.compreahora.com.ar/categoria/perfumeria#" class="shopping-list-icon"><img alt="Axe Musk"></a>
              <h3><a href="/producto/desodorante-para-hombre-axe-musk-musk-en-aerosol-150-ml">Desodorante para hombre Axe Musk musk en aerosol 150 ml</a></h3>
              <span class="price">$ 3.744,05</span>
            </div>
            <div class="card">
              <a href="javascript:void(0)"><img alt="Dove"></a>
              <h3><a href="/producto/antitranspirante-pomelo-1-4-crema-humectante-dove-en-aerosol-150-ml">Antitranspirante pomelo 1/4 crema humectante Dove en aerosol 150 ml</a></h3>
              <span class="price">$ 4.564,91</span>
            </div>
          </div>
        </body></html>
        "##;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(
            detection.products[0].url.as_deref(),
            Some("/producto/desodorante-axe-gold-vainilla-en-aerosol-150-ml")
        );
        assert_eq!(
            detection.products[1].url.as_deref(),
            Some("/producto/desodorante-para-hombre-axe-musk-musk-en-aerosol-150-ml")
        );
    }

    #[test]
    fn size_helpers_recognize_units() {
        assert_eq!(find_size_in_text("Gold Fresh Couture EDP"), None);
        assert_eq!(find_size_in_text("Crystal Emerald EDP"), None);
        assert_eq!(find_size_in_text("Dylan Blush Pink EDP 100 ml"), Some("100 ml".into()));
        assert_eq!(find_size_in_text("light blue homme edp 50"), None);
        assert_eq!(find_size_in_text("PAULVIC WOMAN X50ML"), Some("50ML".into()));
        assert_eq!(find_size_in_text("132 g"), Some("132 g".into()));
        assert_eq!(find_size_in_text("100 Ml"), Some("100 Ml".into()));
        assert_eq!(find_size_in_text("Promo 100ml"), Some("100ml".into()));
        assert_eq!(find_size_in_text("fresh-gold-edp-precio-promocional-100ml/p"), Some("100ml".into()));
        assert!(has_size("Blue Jeans EDT 75 ml"));
        assert!(!has_size("Funny EDT Ed. Limitada"));
        assert!(has_trailing_bare_number("light blue homme edp 50"));
        assert!(!has_trailing_bare_number("One Million EDT"));
    }

    #[test]
    fn size_from_sku_selector_prefers_selected() {
        let html = r##"
        <html><body>
          <div class="product-grid">
            <div class="card">
              <h3><a href="/crystal-emerald">Crystal Emerald EDP</a></h3>
              <div class="skuSelectorContainer">
                <div class="skuSelectorItem skuSelectorItem--50-ml"><span class="valueWrapper">50 ml</span></div>
                <div class="skuSelectorItem skuSelectorItem--90-ml skuSelectorItem--selected"><span class="valueWrapper">90 ml</span></div>
              </div>
              <span class="price">$ 328.000</span>
            </div>
            <div class="card">
              <h3><a href="/dylan-blush">Dylan Blush Pink EDP 100 ml</a></h3>
              <span class="price">$ 328.000</span>
            </div>
            <div class="card">
              <h3><a href="/blue-jeans">Blue Jeans EDT 75 ml</a></h3>
              <span class="price">$ 79.990</span>
            </div>
          </div>
        </body></html>
        "##;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.products[0].name, "Crystal Emerald EDP 90 ml");
    }

    #[test]
    fn size_from_url_when_no_sku_selector() {
        let html = r##"
        <html><body>
          <div class="product-grid">
            <div class="card">
              <h3><a href="/funny-edt-100ml-ed-limitada-1/p">Funny EDT Ed. Limitada</a></h3>
              <span class="price">$ 94.340</span>
            </div>
            <div class="card">
              <h3><a href="/plain-product/p">Plain Product</a></h3>
              <span class="price">$ 50.000</span>
            </div>
            <div class="card">
              <h3><a href="/fresh-gold-100ml/p">Fresh Gold EDP 100 ml</a></h3>
              <span class="price">$ 95.400</span>
            </div>
          </div>
        </body></html>
        "##;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.products[0].name, "Funny EDT Ed. Limitada 100ml");
        // No size in URL and no trailing number -> name unchanged.
        assert_eq!(detection.products[1].name, "Plain Product");
    }

    #[test]
    fn trailing_bare_number_gets_ml() {
        let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card"><a href="/p/light-blue-homme-edp-50.html">light blue homme edp
              50</a><span class="price">$242.100</span></div>
            <div class="card"><a href="/p/one-million.html">One Million EDT</a><span class="price">$266.901</span></div>
            <div class="card"><a href="/p/paula-aura-edt-100.html">paula aura edt 100</a><span class="price">$39.060</span></div>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.products[0].name, "light blue homme edp 50 ml");
        assert_eq!(detection.products[1].name, "One Million EDT");
        assert_eq!(detection.products[2].name, "paula aura edt 100 ml");
    }

    #[test]
    fn existing_size_is_left_untouched() {
        let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card"><a href="/p/dylan">Dylan Blush Pink EDP 100 ml + Neceser</a><span class="price">$328.000</span></div>
            <div class="card"><a href="/p/axe">Desodorante Axe Gold 150 ml</a><span class="price">$3.744</span></div>
            <div class="card"><a href="/p/rexona">Rexona 132 g</a><span class="price">$4.489</span></div>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.products[0].name, "Dylan Blush Pink EDP 100 ml + Neceser");
        assert_eq!(detection.products[1].name, "Desodorante Axe Gold 150 ml");
        assert_eq!(detection.products[2].name, "Rexona 132 g");
    }
}



