#![allow(clippy::cognitive_complexity)]

use std::collections::HashSet;

use ego_tree::{NodeId, NodeRef};
use scraper::Html;
use scraper::node::Node;

use super::prices::{classify_div, contains_confident_price};
use super::{Price, Product};

pub(super) fn extract_products(
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
        .filter_map(|(_, price_div_id, prices)| {
            let price = current_price_of(html, price_div_id).or_else(|| prices.last().cloned())?;
            let card_id = card_of(html, price_div_id, container_id);
            let url = product_link(html, card_id);
            Some(Product {
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
            })
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
    matches!(
        unit.to_ascii_lowercase().as_str(),
        "ml" | "g" | "gr" | "l" | "lt"
    )
}

/// Returns the first `N unit` substring in `text` (e.g. `100 ml`, `100ml`,
/// `100 Ml`, `X50ML`, `132 g`), preserving its original spacing/case. Used to
/// detect whether a name already carries its size and to lift the size out of
/// SKU selectors and product URLs.
pub(super) fn find_size_in_text(text: &str) -> Option<String> {
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
pub(super) fn has_size(name: &str) -> bool {
    find_size_in_text(name).is_some()
}

/// Whether `name` ends in a bare number (e.g. `edp 50`) with no unit.
pub(super) fn has_trailing_bare_number(name: &str) -> bool {
    name.split_whitespace()
        .next_back()
        .is_some_and(|token| !token.is_empty() && token.chars().all(|c| c.is_ascii_digit()))
}

/// Lifts the product size out of a VTEX-style SKU selector inside the card.
/// Prefers the option marked `--selected`, falling back to the first one that
/// carries a size.
pub(super) fn size_from_sku_selector(html: &Html, card_id: NodeId) -> Option<String> {
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
pub(super) fn size_from_url(url: &str) -> Option<String> {
    find_size_in_text(url)
}

/// Ensures the extracted product name carries its size. Names that already
/// include a size are returned unchanged. Otherwise the size is taken from the
/// card's SKU selector (selected option first), then from the product URL, and
/// finally by appending `ml` to a trailing bare number (e.g. `edp 50`).
pub(super) fn enrich_name_with_size(
    html: &Html,
    card_id: NodeId,
    name: String,
    url: &str,
) -> String {
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

pub(super) fn find_structured_name(node: &NodeRef<'_, Node>) -> Option<String> {
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

pub(super) fn largest_text_block(node: &NodeRef<'_, Node>) -> Option<String> {
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
