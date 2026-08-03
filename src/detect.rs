#![allow(clippy::cognitive_complexity)]

use std::collections::{HashMap, HashSet};

use ego_tree::NodeId;
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
    if confident.len() >= 2 {
        return None;
    }
    if confident.len() == 1 {
        return Some(confident);
    }
    let bare: Vec<&str> = tokens
        .iter()
        .filter(|t| !has_separator(t) && (2..=7).contains(&t.len()))
        .map(|s| s.as_str())
        .collect();
    if bare.len() == 1 {
        let t = bare[0];
        return Some(vec![Price {
            value: t.parse().ok()?,
            text: t.to_string(),
        }]);
    }
    None
}

fn contains_price(text: &str) -> bool {
    number_tokens(text)
        .iter()
        .any(|t| has_separator(t) || (2..=7).contains(&t.len()))
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
            let price = &prices[0];
            Product {
                name: guess_name(html, *id),
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

fn guess_name(html: &Html, id: NodeId) -> String {
    let Some(node) = html.tree.get(id) else {
        return String::new();
    };
    let mut best_block = String::new();
    for n in node.descendants() {
        match n.value() {
            Node::Element(el) if el.name() == "a" => {
                if let Some(t) = el.attr("title") {
                    let t = t.trim();
                    if !t.is_empty() {
                        return t.to_string();
                    }
                }
                let text: String = n
                    .descendants()
                    .filter_map(|x| match x.value() {
                        Node::Text(t) => Some(&*t.text),
                        _ => None,
                    })
                    .collect();
                let text = text.trim().to_string();
                if !text.is_empty() && !contains_price(&text) {
                    return text;
                }
            }
            Node::Element(el) if el.name() == "img" => {
                if let Some(a) = el.attr("alt") {
                    let a = a.trim();
                    if !a.is_empty() {
                        return a.to_string();
                    }
                }
            }
            Node::Text(t) => {
                let s: &str = t.text.trim();
                if !s.is_empty()
                    && !contains_price(s)
                    && s.chars().count() > best_block.chars().count()
                {
                    best_block = s.to_string();
                }
            }
            _ => {}
        }
    }
    best_block
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
            <div class="card"><span>1,00</span></div>
            <div class="card"><span>2,00</span></div>
          </div>
          <div class="big-grid">
            <div class="card"><span>1,00</span></div>
            <div class="card"><span>2,00</span></div>
            <div class="card"><span>3,00</span></div>
            <div class="card"><span>4,00</span></div>
            <div class="card"><span>5,00</span></div>
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
            <div class="card"><span>499</span></div>
            <div class="card"><span>899</span></div>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.products.len(), 2);
        assert_eq!(detection.products[0].price, 499.0);
    }

    #[test]
    fn two_prices_in_one_div_excluded() {
        let html = r#"
        <html><body>
          <div class="grid">
            <div class="card"><span class="old">19,99</span> <span class="new">12,99</span></div>
            <div class="card"><span class="old">29,99</span> <span class="new">22,99</span></div>
          </div>
        </body></html>
        "#;
        assert!(detect_grid(html).is_none());
    }

    #[test]
    fn thousands_only_price() {
        let html = r#"
        <html><body>
          <div class="grid">
            <div class="card"><span>1,234</span></div>
            <div class="card"><span>5.678</span></div>
          </div>
        </body></html>
        "#;
        let detection = detect_grid(html).expect("grid should be detected");
        assert_eq!(detection.products[0].price, 1234.0);
        assert_eq!(detection.products[1].price, 5678.0);
    }
}

