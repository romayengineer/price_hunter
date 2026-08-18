//! Pure fuzzy-matching between provider products and canonical products
//! (and, for brand assignment, between provider product names and the brand
//! table). No PocketBase types here — the store feeds in plain rows and
//! persists the results.

/// Minimum score for a provider product to be linked to a canonical product.
pub const MIN_SCORE: f64 = 0.6;

/// Minimum `brand_coverage` for a provider product name to be assigned a
/// brand: every token of the brand must appear in the name.
pub const BRAND_MIN_SCORE: f64 = 1.0;

/// Joins brand, name and size into one comparison string, skipping empties.
/// The parts are space-separated so the token-based normalization in
/// `similarity` treats them as one order-insensitive bag of tokens.
pub fn full_name(brand: &str, name: &str, size: &str) -> String {
    [brand, name, size]
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// One scored (provider product, canonical product) comparison.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchCandidate {
    /// The provider product's id.
    pub provider_product_id: String,
    /// The canonical product's id.
    pub product_id: String,
    /// The similarity score (0.0–1.0).
    pub score: f64,
}

/// Lowers the name, splits it into tokens on non-alphanumeric boundaries and
/// returns the sorted tokens joined by spaces. Sorting makes the comparison
/// order-insensitive, so "Light Blue Homme EDP 50" and "EDP 50 Light Blue
/// Homme" normalize to the same string.
pub(crate) fn normalize(name: &str) -> String {
    let mut tokens: Vec<String> = name
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect();
    tokens.sort();
    tokens.join(" ")
}

/// Sørensen-Dice similarity between two product names (0.0–1.0).
pub fn similarity(a: &str, b: &str) -> f64 {
    strsim::sorensen_dice(&normalize(a), &normalize(b))
}

/// Fraction of the brand's normalized tokens that appear in `name`
/// (0.0–1.0). Used to detect a brand embedded in a long product name, where
/// Sørensen-Dice scores too low (the brand is a small slice of the whole).
/// Example: `brand_coverage("kevin black edt 100 ml", "kevin") == 1.0`.
pub fn brand_coverage(name: &str, brand: &str) -> f64 {
    let name_tokens: Vec<String> = normalize(name).split(' ').map(str::to_owned).collect();
    let brand_tokens: Vec<String> = normalize(brand).split(' ').map(str::to_owned).collect();
    if brand_tokens.is_empty() {
        return 0.0;
    }
    let present = brand_tokens
        .iter()
        .filter(|t| name_tokens.contains(t))
        .count();
    present as f64 / brand_tokens.len() as f64
}

/// Returns the best-scoring `(candidate_id, candidate_text, score)` for
/// `query` using `score`, above `threshold`. Ties go to the longer candidate
/// (more specific). Used for brand assignment; the product matcher shares the
/// same `normalize`/`similarity` core.
pub fn best_match<'a>(
    query: &str,
    candidates: &'a [(String, String)],
    score: impl Fn(&str, &str) -> f64,
    threshold: f64,
) -> Option<(&'a str, &'a str, f64)> {
    candidates
        .iter()
        .filter_map(|(id, text)| {
            let s = score(query, text);
            (s >= threshold).then_some((id.as_str(), text.as_str(), s))
        })
        .max_by(|a, b| {
            a.2.partial_cmp(&b.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.len().cmp(&b.1.len()))
        })
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Recognized product-size units, case-insensitive.
fn is_size_unit(unit: &str) -> bool {
    matches!(
        unit.to_ascii_lowercase().as_str(),
        "ml" | "g" | "gr" | "l" | "lt"
    )
}

/// Normalizes a detected size: ml variants become `N ml`, other units keep
/// their own unit (`132 g` → `132 g`).
fn normalize_size(number: &str, unit: &str) -> String {
    let unit = unit.to_ascii_lowercase();
    if unit == "ml" {
        format!("{number} ml")
    } else {
        format!("{number} {unit}")
    }
}

/// Scans a number (and optional unit) starting at index `i`, returning the
/// number's start, the normalized size, and the index just past it when it is
/// a recognizable size.
#[allow(clippy::cognitive_complexity)]
fn size_at(chars: &[char], i: usize) -> Option<(usize, String, usize)> {
    let num_start = i;
    let mut i = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    let number: String = chars[num_start..i].iter().collect();
    let mut j = i;
    if j < chars.len() && chars[j].is_whitespace() {
        j += 1;
    }
    let unit_start = j;
    while j < chars.len() && chars[j].is_ascii_alphabetic() {
        j += 1;
    }
    if unit_start < j {
        let unit: String = chars[unit_start..j].iter().collect();
        if is_size_unit(&unit) {
            return Some((num_start, normalize_size(&number, &unit), j));
        }
    } else if j == chars.len() || chars[j..].iter().all(|c| c.is_whitespace()) {
        return Some((num_start, format!("{number} ml"), j));
    }
    None
}

/// Strips a trailing size out of `name` and returns it normalized. Recognized:
/// `100 ml`, `100ml`, `100 Ml`, `X50ML`, `132 g`, and a bare trailing number
/// (`edp 50` → `50 ml`). Returns `(name_without_size, Some(size))`, or
/// `(name, None)` when no size is present.
pub fn split_size(name: &str) -> (String, Option<String>) {
    let chars: Vec<char> = name.chars().collect();
    let mut last: Option<(usize, String)> = None;
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit()
            && let Some((start, size, next)) = size_at(&chars, i)
        {
            last = Some((start, size));
            i = next;
            continue;
        }
        i += 1;
    }
    match last {
        Some((start, size)) => {
            let before: String = chars[..start].iter().collect();
            (collapse_whitespace(&before), Some(size))
        }
        None => (name.to_string(), None),
    }
}

/// Removes `brand` from `name` (case-insensitive), collapsing the leftover
/// whitespace. An empty brand returns the name untouched.
pub fn strip_brand(name: &str, brand: &str) -> String {
    if brand.is_empty() {
        return collapse_whitespace(name);
    }
    let brand_lower: Vec<char> = brand.chars().map(|c| c.to_ascii_lowercase()).collect();
    let name_chars: Vec<char> = name.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < name_chars.len() {
        if i + brand_lower.len() <= name_chars.len()
            && name_chars[i..i + brand_lower.len()]
                .iter()
                .map(|c| c.to_ascii_lowercase())
                .eq(brand_lower.iter().copied())
        {
            i += brand_lower.len();
            out.push(' ');
        } else {
            out.push(name_chars[i]);
            i += 1;
        }
    }
    collapse_whitespace(&out.into_iter().collect::<String>())
}

/// Greedily assigns canonical products to provider products within one
/// provider group: sorts candidates by score (highest first) and claims a
/// provider product iff it is not yet assigned and the product has not already
/// been claimed by another provider product in the group. Returns the winning
/// candidates.
pub fn assign_group(candidates: &[MatchCandidate]) -> Vec<MatchCandidate> {
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut claimed_products: Vec<String> = Vec::new();
    let mut winners = Vec::new();
    for candidate in sorted {
        let product_claimed = claimed_products.contains(&candidate.product_id);
        let provider_assigned = winners
            .iter()
            .any(|w: &MatchCandidate| w.provider_product_id == candidate.provider_product_id);
        if !product_claimed && !provider_assigned {
            claimed_products.push(candidate.product_id.clone());
            winners.push(candidate);
        }
    }
    winners
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;

    #[test]
    fn normalize_is_case_and_order_insensitive() {
        assert_eq!(
            normalize("Light Blue Homme EDP 50"),
            normalize("edp 50 light blue homme")
        );
        assert_eq!(
            normalize("Adn Neroli Ectasy!!"),
            normalize("adn neroli ectasy")
        );
    }

    #[test]
    fn similarity_handles_word_reordering() {
        let score = similarity("Light Blue Homme EDP 50", "EDP 50 Light Blue Homme");
        assert!(
            score >= 0.9,
            "reordered words should score high, got {score}"
        );
    }

    #[test]
    fn similarity_lowers_for_different_names() {
        let score = similarity("adn neroli ecstasy", "rose spicy edp");
        assert!(
            score < MIN_SCORE,
            "unrelated names should score low, got {score}"
        );
    }

    #[test]
    fn full_name_joins_brand_name_and_size() {
        assert_eq!(
            full_name("yves saint laurent", "y l'elixir edp", "60 ml"),
            "yves saint laurent y l'elixir edp 60 ml"
        );
    }

    #[test]
    fn full_name_skips_missing_parts() {
        assert_eq!(
            full_name("diesel", "fuel for life edt", ""),
            "diesel fuel for life edt"
        );
        assert_eq!(
            full_name("", "adn neroli ecstasy", ""),
            "adn neroli ecstasy"
        );
    }

    #[test]
    fn full_name_trims_whitespace_parts() {
        assert_eq!(full_name("  ", "  a  ", "   "), "a");
    }

    #[test]
    fn split_size_extracts_trailing_ml_size() {
        assert_eq!(split_size("Gold Fresh Couture EDP 100 Ml"), ("Gold Fresh Couture EDP".to_string(), Some("100 ml".into())));
        assert_eq!(split_size("edp 50"), ("edp".to_string(), Some("50 ml".into())));
        assert_eq!(split_size("PAULVIC WOMAN X50ML"), ("PAULVIC WOMAN X".to_string(), Some("50 ml".into())));
        assert_eq!(split_size("132 g"), ("".to_string(), Some("132 g".into())));
        assert_eq!(split_size("One Million EDT"), ("One Million EDT".to_string(), None));
        // A bare number in the middle is not a size.
        assert_eq!(
            split_size("set 212 men edt 100 + deo"),
            ("set 212 men edt 100 + deo".to_string(), None)
        );
    }

    #[test]
    fn strip_brand_removes_case_insensitively() {
        assert_eq!(strip_brand("Adidas Vibes Smooth Pace", "adidas"), "Vibes Smooth Pace");
        assert_eq!(
            strip_brand("Dolce & Gabbana Original EDT", "dolce & gabbana"),
            "Original EDT"
        );
        assert_eq!(strip_brand("Carolina Herrera 212 Vip", "carolina herrera"), "212 Vip");
        assert_eq!(strip_brand("Plain Name", "diesel"), "Plain Name");
        assert_eq!(strip_brand("  spaced   out  ", ""), "spaced out");
    }

    #[test]
    fn full_name_improves_score_vs_name_only() {
        let brand_name_score = similarity(
            "EDT Diesel Fuel For Life x 125 ml",
            &full_name("diesel", "fuel for life edt", "125 ml"),
        );
        let name_only_score = similarity("EDT Diesel Fuel For Life x 125 ml", "fuel for life edt");
        assert!(
            brand_name_score > name_only_score,
            "including brand+size should score higher, got {brand_name_score} vs {name_only_score}"
        );
    }

    #[test]
    fn assign_group_picks_highest_and_does_not_reuse_products() {
        let candidates = vec![
            MatchCandidate {
                provider_product_id: "pp1".into(),
                product_id: "p1".into(),
                score: 0.8,
            },
            MatchCandidate {
                provider_product_id: "pp1".into(),
                product_id: "p2".into(),
                score: 0.9,
            },
            MatchCandidate {
                provider_product_id: "pp2".into(),
                product_id: "p1".into(),
                score: 0.95,
            },
        ];
        let winners = assign_group(&candidates);
        assert_eq!(
            winners,
            vec![
                MatchCandidate {
                    provider_product_id: "pp2".into(),
                    product_id: "p1".into(),
                    score: 0.95
                },
                MatchCandidate {
                    provider_product_id: "pp1".into(),
                    product_id: "p2".into(),
                    score: 0.9
                },
            ]
        );
    }

    #[test]
    fn brand_coverage_matches_embedded_brand() {
        assert_eq!(brand_coverage("Kevin Black EDT 100 Ml", "kevin"), 1.0);
        assert_eq!(
            brand_coverage("Puro Giesso Mujer EDT 100 Ml", "giesso"),
            1.0
        );
        assert_eq!(
            brand_coverage(
                "adolfo dominguez adn neroli ecstasy 100 ml",
                "adolfo dominguez"
            ),
            1.0
        );
        assert_eq!(brand_coverage("some unrelated name", "diesel"), 0.0);
        // one of two brand tokens present
        assert_eq!(brand_coverage("adolfo neroli", "adolfo dominguez"), 0.5);
        // name carries no brand token
        assert_eq!(brand_coverage("diesel", "adolfo dominguez"), 0.0);
    }

    #[test]
    fn best_match_picks_highest_score_and_longest_tie_break() {
        let candidates = vec![
            ("b1".to_string(), "adolfo".to_string()),
            ("b2".to_string(), "adolfo dominguez".to_string()),
            ("b3".to_string(), "diesel".to_string()),
        ];
        let (id, text, score) = best_match(
            "adolfo dominguez adn neroli ecstasy 100 ml",
            &candidates,
            brand_coverage,
            BRAND_MIN_SCORE,
        )
        .expect("brand should match");
        assert_eq!(id, "b2");
        assert_eq!(text, "adolfo dominguez");
        assert_eq!(score, 1.0);

        assert!(
            best_match(
                "completely unrelated",
                &candidates,
                brand_coverage,
                BRAND_MIN_SCORE
            )
            .is_none()
        );
    }
}
