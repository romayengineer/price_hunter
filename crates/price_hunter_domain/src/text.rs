//! Pure string helpers shared by the detection pipeline. No HTML, no I/O:
//! price tokenizing/parsing, price-div classification, size detection, and
//! link hygiene. The DOM traversal in `price_hunter_core::domain::detect`
//! calls these primitives.

#![allow(clippy::cognitive_complexity)]

/// A price parsed out of raw text: numeric value plus the verbatim token.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPrice {
    /// The parsed numeric value.
    pub value: f64,
    /// The raw price token as it appeared in the text.
    pub text: String,
}

/// Collapses all whitespace runs to single spaces and trims.
pub fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Classifies a div's own text as prices, mirroring the detection pipeline:
/// confident tokens (with separators) win; otherwise a single bare 2–7 digit
/// token with no other content counts. Returns `None` when not price-like.
pub fn classify_text(text: &str) -> Option<Vec<ParsedPrice>> {
    let tokens = number_tokens(text);
    let confident: Vec<ParsedPrice> = tokens
        .iter()
        .filter(|t| has_separator(t))
        .filter_map(|t| {
            parse_price(t).map(|value| ParsedPrice {
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
        return Some(vec![ParsedPrice {
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

/// Whether `text` contains any confident (separator-bearing) price token.
pub fn contains_confident_price(text: &str) -> bool {
    number_tokens(text).iter().any(|t| has_separator(t))
}

fn has_separator(token: &str) -> bool {
    token
        .chars()
        .any(|c| matches!(c, ' ' | '\u{a0}' | '.' | ',' | '\''))
}

/// Splits raw text into candidate number tokens, keeping `.`/`,`/`'` and
/// space-thousands (`1 234`) inside the token when followed by digits.
pub fn number_tokens(text: &str) -> Vec<String> {
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
            } else if matches!(c, '.' | ',' | '\'')
                && j + 1 < chars.len()
                && chars[j + 1].is_ascii_digit()
            {
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

/// Parses one number token, handling `242.100` (thousands), `12,99`
/// (decimal), `1.234,56`, and bare integers.
pub fn parse_price(token: &str) -> Option<f64> {
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
    s[from + 1..].chars().filter(|c| c.is_ascii_digit()).count()
}

/// Recognized product-size units, case-insensitive.
pub fn is_size_unit(unit: &str) -> bool {
    matches!(
        unit.to_ascii_lowercase().as_str(),
        "ml" | "g" | "gr" | "l" | "lt"
    )
}

/// Returns the first `N unit` substring in `text` (e.g. `100 ml`, `100ml`,
/// `100 Ml`, `X50ML`, `132 g`), preserving its original spacing/case. Used to
/// detect whether a name already carries its size and to lift the size out of
/// SKU selectors and product URLs.
pub fn find_size_in_text(text: &str) -> Option<String> {
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
pub fn has_size(name: &str) -> bool {
    find_size_in_text(name).is_some()
}

/// Whether `name` ends in a bare number (e.g. `edp 50`) with no unit.
pub fn has_trailing_bare_number(name: &str) -> bool {
    name.split_whitespace()
        .next_back()
        .is_some_and(|token| !token.is_empty() && token.chars().all(|c| c.is_ascii_digit()))
}

/// Lifts the product size out of a product URL slug (e.g. `...-100ml-...`).
pub fn size_from_url(url: &str) -> Option<String> {
    find_size_in_text(url)
}

/// True for links that don't point anywhere useful for a product page:
/// `#` anchors, `javascript:` stubs, `mailto:`/`tel:`, and bare fragment
/// links (e.g. `https://site/category#`). Those are usually icon/button
/// links that appear before the real product link in the card.
pub fn is_placeholder_href(href: &str) -> bool {
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

/// Escapes a value for use inside a PocketBase filter string literal. Single
/// quotes and backslashes must be backslash-escaped or the filter parses
/// wrong (e.g. `name='A Drop d'Issey...'` → HTTP 400), which used to
/// abort the whole save and silently drop the rest of a capture.
pub fn escape_filter(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

/// Lowercases `s` and replaces non-ASCII characters with their closest ASCII
/// match: accented Latin letters lose their diacritics (`bambú` → `bambu`,
/// `Benoît` → `benoit`), curly quotes and acute accents become `'`, and zero-
/// width / BOM characters are dropped. Characters with no ASCII equivalent are
/// left unchanged. Used both as the sort key and for the CSV output, so a
/// lowercase, ASCII-only file round-trips deterministically and sorts by the
/// base letters (`bambú` and `bambu` compare equal, then the rest of the name
/// decides the order).
pub fn ascii_fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        fold_char(c, &mut out);
    }
    out
}

/// Appends the lowercase ASCII fold of `c` (possibly several chars, e.g.
/// `ß` → `ss`, or none for zero-width marks) to `out`.
fn fold_char(c: char, out: &mut String) {
    match c {
        '\u{feff}' | '\u{200b}' | '\u{200c}' | '\u{200d}' => {}
        c if c.is_ascii() => out.push(c.to_ascii_lowercase()),
        c => match c.to_lowercase().to_string().as_str() {
            "à" | "á" | "â" | "ã" | "ä" | "å" => out.push('a'),
            "è" | "é" | "ê" | "ë" => out.push('e'),
            "ì" | "í" | "î" | "ï" => out.push('i'),
            "ò" | "ó" | "ô" | "õ" | "ö" | "ø" => out.push('o'),
            "ù" | "ú" | "û" | "ü" => out.push('u'),
            "ñ" => out.push('n'),
            "ç" => out.push('c'),
            "ß" => out.push_str("ss"),
            "ÿ" => out.push('y'),
            "æ" => out.push_str("ae"),
            "œ" => out.push_str("oe"),
            "\u{00b4}" | "\u{2018}" | "\u{2019}" | "\u{201a}" | "\u{201b}" | "\u{02b9}"
            | "\u{02bc}" => out.push('\''),
            _ => out.push(c),
        },
    }
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_formats() {
        assert_eq!(parse_price("8.190"), Some(8190.0));
        assert_eq!(parse_price("12,99"), Some(12.99));
        assert_eq!(parse_price("1.234,56"), Some(1234.56));
        assert_eq!(parse_price("499"), Some(499.0));
    }

    #[test]
    fn classifies_confident_and_bare() {
        let prices = classify_text("$8.190").expect("price should be found");
        assert_eq!(prices.len(), 1);
        assert_eq!(prices[0].value, 8190.0);
        assert_eq!(number_tokens("$"), Vec::<String>::new());
        assert!(classify_text("No prices here.").is_none());
    }

    #[test]
    fn sizes_behave() {
        assert_eq!(find_size_in_text("Dylan Blush Pink EDP 100 ml"), Some("100 ml".into()));
        assert_eq!(find_size_in_text("Crystal Emerald EDP"), None);
        assert!(has_size("Blue Jeans EDT 75 ml"));
        assert!(!has_size("Funny EDT Ed. Limitada"));
        assert!(has_trailing_bare_number("light blue homme edp 50"));
        assert!(!has_trailing_bare_number("One Million EDT"));
        assert_eq!(
            size_from_url("fresh-gold-edp-precio-promocional-100ml/p"),
            Some("100ml".into())
        );
    }

    #[test]
    fn placeholders_detected() {
        assert!(is_placeholder_href("#"));
        assert!(is_placeholder_href("javascript:void(0)"));
        assert!(is_placeholder_href("https://site/category#"));
        assert!(!is_placeholder_href("/producto/axe-gold-150-ml"));
    }

    #[test]
    fn escape_filter_handles_apostrophes_and_backslashes() {
        assert_eq!(escape_filter("plain"), "plain");
        assert_eq!(escape_filter("A Drop d'Issey"), "A Drop d\\'Issey");
        assert_eq!(escape_filter(r"a\b"), r"a\\b");
        assert_eq!(escape_filter(r"back\'slash"), r"back\\\'slash");
    }

    #[test]
    fn ascii_fold_lowercases_and_transliterates() {
        assert_eq!(ascii_fold("Bambú"), "bambu");
        assert_eq!(ascii_fold("Agua de Bambú EDT"), "agua de bambu edt");
        assert_eq!(ascii_fold("Benoît"), "benoit");
        assert_eq!(ascii_fold("José Ñoño"), "jose nono");
        assert_eq!(ascii_fold("François Straße"), "francois strasse");
        assert_eq!(ascii_fold("A’B"), "a'b");
        assert_eq!(ascii_fold("\u{feff}abc\u{200b}"), "abc");
        assert_eq!(ascii_fold("30° C"), "30° c");
    }
}
