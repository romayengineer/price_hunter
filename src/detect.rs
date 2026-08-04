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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Product {
    pub name: String,
    pub price_text: String,
    pub price: f64,
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
    merge_price_groups(html, &own, leaf)
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
    let price_set: HashSet<NodeId> = price_divs.iter().map(|(id, _)| *id).collect();
    let mut divs: HashMap<NodeId, usize> = HashMap::new();
    let mut prices: HashMap<NodeId, usize> = HashMap::new();
    let mut best: Option<(NodeId, usize, usize)> = None;
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
        if is_div(el) && p >= 2 {
            let density = p as f64 / d as f64;
            let better = match &best {
                None => true,
                Some((_, bp, bd)) => {
                    let base = *bp as f64 / *bd as f64;
                    density > base || (density == base && (p > *bp || (p == *bp && d < *bd)))
                }
            };
            if better {
                best = Some((id, p, d));
            }
        }
    }
    best.map(|(id, p, _)| (id, p))
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
    price_divs
        .iter()
        .filter(|(id, _)| is_descendant_of(html, *id, container_id))
        .map(|(id, prices)| {
            let price = prices.last().expect("price div has a price");
            Product {
                name: guess_name(html, *id, container_id),
                price_text: price.text.clone(),
                price: price.value,
            }
        })
        .collect()
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

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_structured_name(node: &NodeRef<'_, Node>) -> Option<String> {
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
    fn two_prices_card_picks_current_price() {
        let html = r#"
        <html><body>
          <div class="grid">
            <div class="card"><a>Alpha</a><span class="old">19,99</span> <span class="new">12,99</span></div>
            <div class="card"><a>Beta</a><span class="old">29,99</span> <span class="new">22,99</span></div>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.products.len(), 2);
        assert_eq!(detection.products[0].price, 12.99);
        assert_eq!(detection.products[1].price, 22.99);
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
    fn woocommerce_decimal_comma() {
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
}



