#![allow(clippy::cognitive_complexity)]

use std::collections::{HashMap, HashSet};

use ego_tree::{NodeId, NodeRef};
use scraper::Html;
use scraper::node::Node;

use super::prices::classify_div;
use price_hunter_domain::matching::{
    BRAND_MIN_SCORE, best_match, brand_coverage, brand_from_name, enrich_brand_with_opt,
    is_valid_brand_text,
};
use price_hunter_domain::model::{Price, Product};
use price_hunter_domain::text::{self as domain_text, collapse_whitespace};

pub(super) fn extract_products(
    html: &Html,
    container_id: NodeId,
    price_divs: &[(NodeId, Vec<Price>)],
) -> Vec<Product> {
    extract_products_inner(html, container_id, price_divs, None)
}

pub(super) fn extract_products_with_brands(
    html: &Html,
    container_id: NodeId,
    price_divs: &[(NodeId, Vec<Price>)],
    brands: &[String],
) -> Vec<Product> {
    extract_products_inner(html, container_id, price_divs, Some(brands))
}

fn extract_products_inner(
    html: &Html,
    container_id: NodeId,
    price_divs: &[(NodeId, Vec<Price>)],
    brands: Option<&[String]>,
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
    let trusted = brands
        .filter(|b| !b.is_empty())
        .map(|brands| learn_trusted_signatures(html, &cards, container_id, brands))
        .unwrap_or_default();
    let brand_candidates: Option<Vec<(String, String)>> = brands.map(|b| {
        b.iter()
            .enumerate()
            .map(|(i, name)| (format!("b{i}"), name.clone()))
            .collect()
    });
    cards
        .into_iter()
        .filter_map(|(_, price_div_id, prices)| {
            let price = current_price_of(html, price_div_id).or_else(|| prices.last().cloned())?;
            let card_id = card_of(html, price_div_id, container_id);
            let url = product_link(html, card_id);
            let base_name = guess_name(html, price_div_id, container_id);
            let sized_name =
                enrich_name_with_size(html, card_id, base_name, url.as_deref().unwrap_or(""));
            let (brand, final_name) = if let Some(cands) = brand_candidates.as_deref() {
                let brand_opt = extract_brand_for_card(html, card_id, &sized_name, cands, &trusted);
                enrich_brand_with_opt(sized_name, brand_opt)
            } else {
                let b = brand_of(html, card_id);
                enrich_brand_with_opt(sized_name, b)
            };
            Some(Product {
                name: final_name,
                price_text: price.text.clone(),
                price: price.value,
                url,
                images: card_images(html, card_id),
                currency: detect_currency(html, card_id),
                brand,
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
            || el.attr("data-price-type") == Some("finalPrice")
            || el.attr("data-id") == Some("spot-price");
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

use price_hunter_domain::text::{find_size_in_text, size_from_url};

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

/// Lifts the size out of a FastStore / generic card (juleriaque):
/// looks for `data-fs-product-card-sku-variant` or `data-id="presentation-variants"`
/// or any short text inside the card that matches a size pattern.
fn size_from_generic_card(html: &Html, card_id: NodeId) -> Option<String> {
    let node = html.tree.get(card_id)?;
    for n in node.descendants() {
        let Node::Element(el) = n.value() else {
            continue;
        };
        // FastStore variant: data-fs-product-card-sku-variant or presentation-variants container
        let is_variant = el.attr("data-fs-product-card-sku-variant").is_some()
            || el.attr("data-id") == Some("presentation-variants")
            || el.attr("data-fs-product-card-sku-variants").is_some();
        if !is_variant {
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
        if let Some(size) = find_size_in_text(&text) {
            return Some(size);
        }
    }
    // fallback: any descendant short text that looks like a size and is not a price
    for n in node.descendants() {
        let Node::Element(el) = n.value() else {
            continue;
        };
        // skip price-like elements
        if el.attr("data-id") == Some("spot-price") || el.attr("data-id") == Some("list-price") {
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
        if text.chars().count() > 16 {
            continue;
        }
        if domain_text::contains_confident_price(&text) {
            continue;
        }
        if let Some(size) = find_size_in_text(&text) {
            return Some(size);
        }
    }
    None
}

/// Ensures the extracted product name carries its size. Names that already
/// include a size are returned unchanged. Otherwise the size is taken from the
/// card's SKU selector (selected option first), then from a generic FastStore
/// variant, then from the product URL, and finally by appending `ml` to a
/// trailing bare number (e.g. `edp 50`).
pub(super) fn enrich_name_with_size(
    html: &Html,
    card_id: NodeId,
    name: String,
    url: &str,
) -> String {
    if domain_text::has_size(&name) {
        return name;
    }
    if let Some(size) = size_from_sku_selector(html, card_id) {
        return format!("{name} {size}");
    }
    if let Some(size) = size_from_generic_card(html, card_id) {
        return format!("{name} {size}");
    }
    if let Some(size) = size_from_url(url) {
        return format!("{name} {size}");
    }
    if domain_text::has_trailing_bare_number(&name) {
        return format!("{name} ml");
    }
    name
}

/// Prepends the brand the site renders inside the card (as a separate element)
/// to the extracted name. Names that already carry the brand — every brand
/// token present in the name, case-insensitive — are left unchanged, so
/// sites that repeat the brand in the product title ("…Dove…", "Adidas …")
/// don't get a duplicate.
#[allow(dead_code)]
pub(super) fn enrich_name_with_brand(html: &Html, card_id: NodeId, name: String) -> String {
    let Some(brand) = brand_of(html, card_id) else {
        return name;
    };
    if brand_coverage(&name, &brand) >= 1.0 {
        return name;
    }
    format!("{brand} {name}")
}

/// Finds the brand text rendered inside the product card. Recognized markup:
/// VTEX (`productBrandName` / `productBrandContainer`), Magento/Hyva
/// (`product-item-brand`), and `__brand`-suffixed headings (e.g. compreahora's
/// `list-item-ar-list-item__brand-HYP`). The VTEX product-name spans
/// (`productBrand` / `brandName`) are deliberately not brand elements.
fn brand_of(html: &Html, card_id: NodeId) -> Option<String> {
    let node = html.tree.get(card_id)?;
    for n in node.descendants() {
        let Node::Element(el) = n.value() else {
            continue;
        };
        if !is_brand_element(el) {
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
        if text.is_empty()
            || !text.chars().any(char::is_alphanumeric)
            || domain_text::contains_confident_price(&text)
            || text.chars().count() > 32
        {
            continue;
        }
        return Some(text);
    }
    None
}

/// Whether `el` looks like a brand element rather than the product name.
/// Recognized: VTEX (`productBrandName` / `productBrandContainer`),
/// Magento/Hyva (`product-item-brand`, or a `<strong class="product brand">`),
/// `__brand`-suffixed headings (e.g. compreahora's
/// `list-item-ar-list-item__brand-HYP`), FastStore `data-id="brand-name"`
/// (juleriaque), and any class containing `brand`/`marca` generically.
/// The VTEX product-name spans (`productBrand` / `brandName`) are
/// deliberately not brand elements.
fn is_brand_element(el: &scraper::node::Element) -> bool {
    if el.attr("data-id") == Some("brand-name") {
        return true;
    }
    if el.attr("data-testid").is_some_and(|v| v.to_ascii_lowercase().contains("brand")) {
        return true;
    }
    let classes: Vec<&str> = el.classes().collect();
    if classes.iter().any(|c| {
        let lc = c.to_ascii_lowercase();
        lc.contains("brand") || lc.contains("marca")
    }) {
        return true;
    }
    classes.iter().any(|c| {
        c.contains("productBrandName")
            || c.contains("productBrandContainer")
            || c.contains("product-item-brand")
            || c.contains("__brand")
    }) || (el.name() == "strong" && classes.contains(&"brand") && classes.contains(&"product"))
}

/// Extracts the brand for one card using catalog + trusted path + heuristic +
/// name-split fallback.
fn extract_brand_for_card(
    html: &Html,
    card_id: NodeId,
    name: &str,
    brand_candidates: &[(String, String)],
    trusted: &HashSet<String>,
) -> Option<String> {
    if let Some(t) = trusted_brand_of(html, card_id, trusted) {
        return Some(t);
    }
    if let Some(b) = brand_of(html, card_id) {
        return Some(b);
    }
    brand_from_name(name, brand_candidates)
}

/// Returns the brand-like text on a trusted signature inside `card`, if any.
/// Validates length/price filters but accepts unknown brands (stored raw).
fn trusted_brand_of(html: &Html, card_id: NodeId, trusted: &HashSet<String>) -> Option<String> {
    if trusted.is_empty() {
        return None;
    }
    let node = html.tree.get(card_id)?;
    // prefer signatures with most hits first - sort trusted by insertion order is not stable,
    // so just iterate; first valid wins.
    for n in node.descendants() {
        let Node::Element(el) = n.value() else {
            continue;
        };
        let sig = signature_of(el);
        if !trusted.contains(&sig) {
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
        if !is_valid_brand_text(&text) {
            continue;
        }
        return Some(text);
    }
    None
}

fn signature_of(el: &scraper::node::Element) -> String {
    let mut classes: Vec<&str> = el.classes().collect();
    classes.sort_unstable();
    let class_part = classes.join(",");
    let data_id = el.attr("data-id").unwrap_or("");
    if data_id.is_empty() {
        format!("{}:{}", el.name(), class_part)
    } else {
        format!("{}:{}:{}", el.name(), class_part, data_id)
    }
}

fn learn_trusted_signatures(
    html: &Html,
    cards: &[(NodeId, NodeId, Vec<Price>)],
    container_id: NodeId,
    brands: &[String],
) -> HashSet<String> {
    let brand_candidates: Vec<(String, String)> = brands
        .iter()
        .enumerate()
        .map(|(i, b)| (format!("b{i}"), b.clone()))
        .collect();
    // we stored key = card_of result, need actual card ids to inspect;
    // recompute from price divs would be ideal, but we have card key already.
    // To avoid extra work, collect distinct keys.
    let card_ids: Vec<NodeId> = cards.iter().map(|(key, _, _)| *key).collect();
    // dedup
    let mut uniq: Vec<NodeId> = Vec::new();
    for id in card_ids {
        if !uniq.contains(&id) {
            uniq.push(id);
        }
    }
    // also include cards derived from container directly if key dedup already covered,
    // fallback to all card keys that are descendants
    if uniq.is_empty() {
        return HashSet::new();
    }
    let total = uniq.len() as f64;
    let mut sig_hits: HashMap<String, usize> = HashMap::new();
    let mut sig_total: HashMap<String, usize> = HashMap::new();
    for card_id in &uniq {
        let Some(node) = html.tree.get(*card_id) else {
            continue;
        };
        let mut seen_sigs: HashSet<String> = HashSet::new();
        for n in node.descendants() {
            let Node::Element(el) = n.value() else {
                continue;
            };
            let sig = signature_of(el);
            if !seen_sigs.insert(sig.clone()) {
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
            if !is_valid_brand_text(&text) {
                continue;
            }
            *sig_total.entry(sig.clone()).or_insert(0) += 1;
            if best_match(&text, &brand_candidates, brand_coverage, BRAND_MIN_SCORE).is_some() {
                *sig_hits.entry(sig).or_insert(0) += 1;
            }
        }
    }
    let mut trusted = HashSet::new();
    for (sig, hits) in sig_hits {
        let h = hits as f64;
        let tot = *sig_total.get(&sig).unwrap_or(&0) as f64;
        if h / total >= 0.25 {
            trusted.insert(sig.clone());
        }
        if tot > 0.0 && h / tot >= 0.5 && h / total >= 0.20 {
            trusted.insert(sig);
        }
    }
    // remove the container id itself if accidentally learned - not needed.
    let _ = container_id;
    trusted
}

#[allow(clippy::cognitive_complexity)] // heuristic DOM walk — splitting hurts readability
pub(super) fn find_structured_name(node: &NodeRef<'_, Node>) -> Option<String> {
    // FastStore / juleriaque: h2[data-id="product-name"] (+ optional h4[data-id="product-description"])
    for n in node.descendants() {
        let Node::Element(el) = n.value() else {
            continue;
        };
        if el.attr("data-id") == Some("product-name") {
            let name: String = n
                .descendants()
                .filter_map(|x| match x.value() {
                    Node::Text(t) => Some(&*t.text),
                    _ => None,
                })
                .collect();
            let name = collapse_whitespace(&name);
            if name.is_empty() || domain_text::contains_confident_price(&name) {
                continue;
            }
            // append description if present nearby in the same card
            let mut desc_text = String::new();
            for m in node.descendants() {
                let Node::Element(e) = m.value() else {
                    continue;
                };
                if e.attr("data-id") == Some("product-description") {
                    let d: String = m
                        .descendants()
                        .filter_map(|x| match x.value() {
                            Node::Text(t) => Some(&*t.text),
                            _ => None,
                        })
                        .collect();
                    let d = collapse_whitespace(&d);
                    if !d.is_empty() && !domain_text::contains_confident_price(&d) {
                        desc_text = d;
                        break;
                    }
                }
            }
            if desc_text.is_empty() {
                return Some(name);
            }
            return Some(format!("{name} {desc_text}"));
        }
    }
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
        if !text.is_empty() && !domain_text::contains_confident_price(&text) {
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
                if !text.is_empty() && !domain_text::contains_confident_price(&text) {
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
        if href.is_empty() || domain_text::is_placeholder_href(href) {
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
                && !domain_text::contains_confident_price(&s)
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
