//! Pure fuzzy-matching between provider products and canonical products
//! (and, for brand assignment, between provider product names and the brand
//! table). No PocketBase types here — the store feeds in plain rows and
//! persists the results.

/// Minimum score for a provider product to be linked to a canonical product.
pub const MIN_SCORE: f64 = 0.6;

/// Minimum `brand_coverage` for a provider product name to be assigned a
/// brand: every token of the brand must appear in the name.
pub const BRAND_MIN_SCORE: f64 = 1.0;

/// Joins brand and name into one comparison string, skipping empties.
/// The parts are space-separated so the token-based normalization in
/// `similarity` treats them as one order-insensitive bag of tokens.
pub fn full_name(brand: &str, name: &str) -> String {
    [brand, name]
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

/// Comparison key for one name, precomputed once per `-match-products` run so
/// the `N * M` pair loop never re-normalizes. `stripped` mirrors exactly what
/// `strsim::sorensen_dice` sees (whitespace removed), and `bigrams` mirrors
/// its bigram multiset, so [`dice_normalized`] agrees with [`similarity`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedName {
    /// `normalize(raw)` (sorted tokens, single-space joined).
    pub normalized: String,
    /// `normalized` with all whitespace removed (what Dice bigrams run over).
    pub stripped: String,
    /// Byte length of `stripped` (`strsim` measures `.len()` in bytes).
    pub byte_len: usize,
    /// Bigram multiset of `stripped` (chars zipped with a one-char offset).
    pub bigrams: Vec<(char, char)>,
}

/// Precomputes the comparison key for one raw name.
pub fn prepare_name(raw: &str) -> PreparedName {
    let normalized = normalize(raw);
    let stripped: String = normalized.chars().filter(|c| !c.is_whitespace()).collect();
    let byte_len = stripped.len();
    let bigrams: Vec<(char, char)> = stripped.chars().zip(stripped.chars().skip(1)).collect();
    PreparedName {
        normalized,
        stripped,
        byte_len,
        bigrams,
    }
}

/// The provider-side comparison key: `name` with its trailing size stripped
/// (falling back to the full name when nothing is left), matching the
/// previous per-pair `split_size` behavior in the backfill.
pub fn pp_match_key(name: &str) -> String {
    let (without_size, _) = split_size(name);
    if without_size.trim().is_empty() {
        name.to_string()
    } else {
        without_size
    }
}

/// Exact Sørensen-Dice over two precomputed keys. Agrees with
/// `strsim::sorensen_dice` on the normalized inputs (same whitespace
/// stripping, same `< 2` byte-length early-out, same multiset intersection).
pub fn dice_normalized(a: &PreparedName, b: &PreparedName) -> f64 {
    if a.stripped == b.stripped {
        return 1.0;
    }
    if a.byte_len < 2 || b.byte_len < 2 {
        return 0.0;
    }
    let mut counts: std::collections::HashMap<(char, char), usize> =
        std::collections::HashMap::new();
    for bigram in &a.bigrams {
        *counts.entry(*bigram).or_insert(0) += 1;
    }
    let mut intersection = 0usize;
    for bigram in &b.bigrams {
        if let Some(count) = counts.get_mut(bigram)
            && *count > 0
        {
            *count -= 1;
            intersection += 1;
        }
    }
    (2 * intersection) as f64 / (a.byte_len + b.byte_len - 2) as f64
}

/// Upper bound on the Dice score: the best two strings of these lengths can
/// do (every bigram of the shorter overlapping). A pair with
/// `max_dice < MIN_SCORE` can never reach the threshold — skipping it loses
/// no recall.
pub fn max_dice(a: &PreparedName, b: &PreparedName) -> f64 {
    if a.stripped == b.stripped {
        return 1.0;
    }
    if a.byte_len < 2 || b.byte_len < 2 {
        return 0.0;
    }
    (2 * a.bigrams.len().min(b.bigrams.len())) as f64 / (a.byte_len + b.byte_len - 2) as f64
}

/// Whether the pair can still reach `MIN_SCORE` on length grounds alone.
pub fn length_passes(a: &PreparedName, b: &PreparedName) -> bool {
    max_dice(a, b) >= MIN_SCORE
}

/// Probe prefix length for the bigram prefix filter: with
/// `tau = ceil(MIN_SCORE * n / 2)` the minimum multiset overlap any
/// `>= MIN_SCORE` pair needs, the first `n - tau + 1` bigrams (rare-first)
/// of the probe are guaranteed to hit the index. Using this lower-bound tau
/// only ever enlarges the probe, so candidate generation stays exact.
pub fn probe_prefix_len(bigram_count: usize) -> usize {
    if bigram_count == 0 {
        return 0;
    }
    let tau = ((MIN_SCORE * bigram_count as f64) / 2.0).ceil() as usize;
    let tau = tau.max(1).min(bigram_count);
    bigram_count - tau + 1
}

/// Canonical key for brand grouping: whitespace-collapsed, case- and
/// accent-folded. Both sides (canonical `products.brand` and the resolved
/// provider brand) go through this function so `"Adidas"` and `"adidas"`
/// land in the same group. Empty input stays empty (= unknown brand).
pub fn brand_key(name: &str) -> String {
    crate::text::ascii_fold(&crate::text::collapse_whitespace(name))
}

/// Whether a (provider product, canonical product) pair survives brand
/// partitioning. Unknown on either side (`None` / `""`) never filters — only
/// two *known, different* brands are excluded. Run `-match-brands` first so
/// provider brands are populated.
pub fn brand_passes(provider_brand: Option<&str>, product_brand: &str) -> bool {
    match provider_brand {
        None | Some("") => true,
        Some(pp) => product_brand.is_empty() || pp == product_brand,
    }
}

/// One provider product with its precomputed comparison key and resolved
/// brand (`None` = unknown brand, compared against every brand group).
#[derive(Clone, Debug)]
pub struct PreparedProvider {
    /// Index into the caller's provider slice (stable for result mapping).
    pub index: usize,
    /// Precomputed key of [`pp_match_key`].
    pub name: PreparedName,
    /// Resolved canonical brand key, or `None` when unknown.
    pub brand: Option<String>,
}

/// One canonical product with its precomputed comparison key and brand group
/// (`""` = unknown brand, compared against every provider).
#[derive(Clone, Debug)]
pub struct PreparedProduct {
    /// Index into the caller's product slice (stable for result mapping).
    pub index: usize,
    /// Precomputed key of the full display name.
    pub name: PreparedName,
    /// Canonical brand key, or `""` when the product has no brand.
    pub brand: String,
}

/// How many pairs each blocking stage removed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockStats {
    /// `providers.len() * products.len()` before blocking.
    pub total_pairs: usize,
    /// Pairs excluded by brand partitioning (two known, different brands).
    pub brand_skipped: usize,
    /// Pairs with no probe-prefix bigram in the index (cannot reach
    /// `MIN_SCORE` by the prefix-filter guarantee).
    pub prefix_skipped: usize,
    /// Pairs that reached the length check but failed it.
    pub length_skipped: usize,
    /// Pairs that reached the exact Dice computation.
    pub scored: usize,
}

/// Generates the exact candidate pairs to score: brand partition + bigram
/// prefix filter, then a length check. Every pair scoring `>= MIN_SCORE`
/// under [`similarity`] is contained in the output — the filters only remove
/// pairs that provably cannot reach the threshold, plus cross-brand pairs
/// (the domain rule: different known brands never link).
///
/// Returns `(candidates, stats)` where each candidate is a
/// `(provider_index, product_index)` pair using the [`PreparedProvider::index`]
/// / [`PreparedProduct::index`] values (not slice positions).
pub fn plan_candidates(
    providers: &[PreparedProvider],
    products: &[PreparedProduct],
) -> (Vec<(usize, usize)>, BlockStats) {
    let mut stats = BlockStats {
        total_pairs: providers.len() * products.len(),
        ..BlockStats::default()
    };
    if providers.is_empty() || products.is_empty() {
        return (Vec::new(), stats);
    }
    let index = BigramIndex::build(products);
    let mut candidates = Vec::new();
    for pp in providers {
        plan_one_provider(pp, products, &index, &mut candidates, &mut stats);
    }
    (candidates, stats)
}

/// Bigram document frequencies plus the full bigram -> product postings over
/// the canonical products. The postings are a superset of any prefix index,
/// so probing them with the provider's rare-first prefix stays exact.
struct BigramIndex {
    doc_freq: std::collections::HashMap<(char, char), usize>,
    postings: std::collections::HashMap<(char, char), Vec<usize>>,
}

impl BigramIndex {
    fn build(products: &[PreparedProduct]) -> Self {
        let mut doc_freq = std::collections::HashMap::new();
        let mut postings: std::collections::HashMap<(char, char), Vec<usize>> =
            std::collections::HashMap::new();
        for (pos, product) in products.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            for bigram in &product.name.bigrams {
                if seen.insert(*bigram) {
                    *doc_freq.entry(*bigram).or_insert(0) += 1;
                    postings.entry(*bigram).or_default().push(pos);
                }
            }
        }
        Self { doc_freq, postings }
    }

    /// Rare-first rank for probe ordering (unseen bigrams sort last; they
    /// match nothing and are dropped from the probe).
    fn rank(&self, bigram: &(char, char)) -> (usize, (char, char)) {
        (
            self.doc_freq.get(bigram).copied().unwrap_or(usize::MAX),
            *bigram,
        )
    }
}

/// Plans the candidates for one provider product: brand universe, then the
/// prefix probe (or exact matching for tiny names), then the length check.
fn plan_one_provider(
    pp: &PreparedProvider,
    products: &[PreparedProduct],
    index: &BigramIndex,
    candidates: &mut Vec<(usize, usize)>,
    stats: &mut BlockStats,
) {
    // Universe under brand partitioning: same-brand + unknown-brand
    // products, or everything when the provider brand is unknown.
    let universe: Vec<usize> = products
        .iter()
        .enumerate()
        .filter(|(_, p)| brand_passes(pp.brand.as_deref(), &p.brand))
        .map(|(pos, _)| pos)
        .collect();
    stats.brand_skipped += products.len() - universe.len();
    if universe.is_empty() {
        return;
    }
    if pp.name.bigrams.is_empty() {
        plan_tiny_provider(pp, products, &universe, candidates, stats);
        return;
    }
    let hits = probe_hits(pp, &universe, index);
    stats.prefix_skipped += universe.len() - hits.len();
    for pos in hits {
        let product = &products[pos];
        if !length_passes(&pp.name, &product.name) {
            stats.length_skipped += 1;
            continue;
        }
        candidates.push((pp.index, product.index));
        stats.scored += 1;
    }
}

/// Tiny names (no bigrams) only match exact-equal strings (Dice 1.0).
fn plan_tiny_provider(
    pp: &PreparedProvider,
    products: &[PreparedProduct],
    universe: &[usize],
    candidates: &mut Vec<(usize, usize)>,
    stats: &mut BlockStats,
) {
    for pos in universe {
        let product = &products[*pos];
        if pp.name.stripped == product.name.stripped {
            candidates.push((pp.index, product.index));
            stats.scored += 1;
        } else {
            stats.length_skipped += 1;
        }
    }
}

/// Union of posting lists for the provider's rare-first probe prefix,
/// restricted to the brand universe.
fn probe_hits(
    pp: &PreparedProvider,
    universe: &[usize],
    index: &BigramIndex,
) -> std::collections::HashSet<usize> {
    // Rare-first multiset order; the probe is the first `probe_prefix_len`
    // entries (duplicates kept for the prefix-size guarantee), looked up as
    // distinct bigrams present in the index.
    let mut ordered = pp.name.bigrams.clone();
    ordered.sort_by_key(|bigram| index.rank(bigram));
    let prefix_len = probe_prefix_len(ordered.len()).min(ordered.len());
    let in_universe: std::collections::HashSet<usize> = universe.iter().copied().collect();
    let mut hits = std::collections::HashSet::new();
    let mut seen_probe = std::collections::HashSet::new();
    for bigram in ordered.into_iter().take(prefix_len) {
        if !index.doc_freq.contains_key(&bigram) || !seen_probe.insert(bigram) {
            continue;
        }
        collect_postings(index, &bigram, &in_universe, &mut hits);
    }
    hits
}

/// Inserts the in-universe products posted under `bigram` into `hits`.
fn collect_postings(
    index: &BigramIndex,
    bigram: &(char, char),
    in_universe: &std::collections::HashSet<usize>,
    hits: &mut std::collections::HashSet<usize>,
) {
    if let Some(posted) = index.postings.get(bigram) {
        for pos in posted {
            if in_universe.contains(pos) {
                hits.insert(*pos);
            }
        }
    }
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

/// Like `enrich_name_with_brand` but takes an explicit `brand` value.
/// Returns `(Some(brand), enriched_name)` when a brand was supplied, otherwise
/// `(None, name)`. The brand is prepended only when not already covered.
pub fn enrich_brand_with_opt(name: String, brand: Option<String>) -> (Option<String>, String) {
    let Some(b) = brand else {
        return (None, name);
    };
    if b.trim().is_empty() {
        return (None, name);
    }
    if brand_coverage(&name, &b) >= 1.0 {
        return (Some(b), name);
    }
    let enriched = format!("{b} {name}");
    (Some(b), enriched)
}

/// Tries to split a brand out of `name` via catalog token coverage.
/// Returns the matched catalog brand text (preserving its original case) when
/// every token of a known brand appears in `name`.
pub fn brand_from_name(name: &str, brand_candidates: &[(String, String)]) -> Option<String> {
    best_match(name, brand_candidates, brand_coverage, BRAND_MIN_SCORE)
        .map(|(_, text, _)| text.to_string())
}

/// Whether card brand text looks like a real brand (non-empty, alphanumeric,
/// no confident price, at most 32 chars).
pub fn is_valid_brand_text(text: &str) -> bool {
    use crate::text::contains_confident_price;
    !text.is_empty()
        && text.chars().any(char::is_alphanumeric)
        && !contains_confident_price(text)
        && text.chars().count() <= 32
}

fn collapse_whitespace(text: &str) -> String {
    crate::text::collapse_whitespace(text)
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

fn consume_digits(chars: &[char], start: usize) -> usize {
    let mut i = start;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    i
}

fn skip_one_space(chars: &[char], idx: usize) -> usize {
    if idx < chars.len() && chars[idx].is_whitespace() {
        idx + 1
    } else {
        idx
    }
}

fn consume_alpha(chars: &[char], start: usize) -> usize {
    let mut j = start;
    while j < chars.len() && chars[j].is_ascii_alphabetic() {
        j += 1;
    }
    j
}

fn is_trailing_whitespace(chars: &[char], from: usize) -> bool {
    from == chars.len() || chars[from..].iter().all(|c| c.is_whitespace())
}

/// Scans a number (and optional unit) starting at index `i`, returning the
/// number's start, the normalized size, and the index just past it when it is
/// a recognizable size.
fn size_at(chars: &[char], i: usize) -> Option<(usize, String, usize)> {
    let num_start = i;
    let num_end = consume_digits(chars, i);
    let number: String = chars[num_start..num_end].iter().collect();
    let unit_start = skip_one_space(chars, num_end);
    let unit_end = consume_alpha(chars, unit_start);
    if unit_start < unit_end {
        let unit: String = chars[unit_start..unit_end].iter().collect();
        if is_size_unit(&unit) {
            return Some((num_start, normalize_size(&number, &unit), unit_end));
        }
        return None;
    }
    if is_trailing_whitespace(chars, unit_end) {
        return Some((num_start, format!("{number} ml"), unit_end));
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

/// Builds a [`MatchCandidate`] from stored row parts (provider product id,
/// canonical product id, score).
pub fn match_candidate(
    provider_product_id: &str,
    product_id: &str,
    score: f64,
) -> MatchCandidate {
    MatchCandidate {
        provider_product_id: provider_product_id.to_string(),
        product_id: product_id.to_string(),
        score,
    }
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
    fn full_name_joins_brand_and_name() {
        assert_eq!(
            full_name("yves saint laurent", "y l'elixir edp"),
            "yves saint laurent y l'elixir edp"
        );
    }

    #[test]
    fn full_name_skips_missing_parts() {
        assert_eq!(
            full_name("diesel", "fuel for life edt"),
            "diesel fuel for life edt"
        );
        assert_eq!(full_name("", "adn neroli ecstasy"), "adn neroli ecstasy");
    }

    #[test]
    fn full_name_trims_whitespace_parts() {
        assert_eq!(full_name("  ", "  a  "), "a");
    }

    #[test]
    fn split_size_extracts_trailing_ml_size() {
        assert_eq!(
            split_size("Gold Fresh Couture EDP 100 Ml"),
            ("Gold Fresh Couture EDP".to_string(), Some("100 ml".into()))
        );
        assert_eq!(
            split_size("edp 50"),
            ("edp".to_string(), Some("50 ml".into()))
        );
        assert_eq!(
            split_size("PAULVIC WOMAN X50ML"),
            ("PAULVIC WOMAN X".to_string(), Some("50 ml".into()))
        );
        assert_eq!(split_size("132 g"), ("".to_string(), Some("132 g".into())));
        assert_eq!(
            split_size("One Million EDT"),
            ("One Million EDT".to_string(), None)
        );
        // A bare number in the middle is not a size.
        assert_eq!(
            split_size("set 212 men edt 100 + deo"),
            ("set 212 men edt 100 + deo".to_string(), None)
        );
    }

    #[test]
    fn strip_brand_removes_case_insensitively() {
        assert_eq!(
            strip_brand("Adidas Vibes Smooth Pace", "adidas"),
            "Vibes Smooth Pace"
        );
        assert_eq!(
            strip_brand("Dolce & Gabbana Original EDT", "dolce & gabbana"),
            "Original EDT"
        );
        assert_eq!(
            strip_brand("Carolina Herrera 212 Vip", "carolina herrera"),
            "212 Vip"
        );
        assert_eq!(strip_brand("Plain Name", "diesel"), "Plain Name");
        assert_eq!(strip_brand("  spaced   out  ", ""), "spaced out");
    }

    #[test]
    fn full_name_improves_score_vs_name_only() {
        let brand_name_score = similarity(
            "EDT Diesel Fuel For Life",
            &full_name("diesel", "fuel for life edt"),
        );
        let name_only_score = similarity("EDT Diesel Fuel For Life", "fuel for life edt");
        assert!(
            brand_name_score > name_only_score,
            "including brand should score higher, got {brand_name_score} vs {name_only_score}"
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
    fn dice_normalized_agrees_with_similarity() {
        let pairs = [
            ("Diesel Fuel For Life EDT 125 ml", "diesel fuel for life edt"),
            ("Light Blue Homme EDP 50", "EDP 50 Light Blue Homme"),
            ("adn neroli ecstasy", "rose spicy edp"),
            ("abcd", "abce"),
            ("a", "a"),
            ("a", "b"),
            ("", ""),
            ("", "nonempty"),
            ("ADN Neroli Ectasy!!", "adn neroli ecstasy 100 ml"),
            ("Kenzo Flower EDP 100 ml", "flower by kenzo edp 100ml"),
            ("Dior Sauvage EDT 100 ml", "sauvage parfum 60 ml"),
        ];
        for (a, b) in pairs {
            let expected = similarity(a, &pp_match_key(b));
            let got = dice_normalized(&prepare_name(a), &prepare_name(&pp_match_key(b)));
            assert!(
                (expected - got).abs() < 1e-9,
                "dice mismatch for {a:?} vs {b:?}: {expected} vs {got}"
            );
            // The length filter must never exclude a pair that reaches the
            // threshold (necessary condition, no recall lost).
            if expected >= MIN_SCORE {
                assert!(
                    length_passes(&prepare_name(a), &prepare_name(&pp_match_key(b))),
                    "length filter excluded {a:?} vs {b:?} scoring {expected}"
                );
            }
        }
    }

    #[test]
    fn pp_match_key_strips_size_with_fallback() {
        assert_eq!(
            pp_match_key("Diesel Fuel For Life EDT 125 ml"),
            "Diesel Fuel For Life EDT"
        );
        assert_eq!(pp_match_key("132 g"), "132 g");
        assert_eq!(pp_match_key("One Million EDT"), "One Million EDT");
    }

    #[test]
    fn brand_key_folds_case_and_accents() {
        assert_eq!(brand_key("Adidas"), brand_key("adidas"));
        assert_eq!(brand_key("  Carolina   Herrera "), "carolina herrera");
        assert_eq!(brand_key(""), "");
    }

    #[test]
    fn brand_passes_only_excludes_two_known_different_brands() {
        assert!(brand_passes(Some("diesel"), "diesel"));
        assert!(brand_passes(None, "diesel"));
        assert!(brand_passes(Some(""), "diesel"));
        assert!(brand_passes(Some("diesel"), ""));
        assert!(brand_passes(None, ""));
        assert!(!brand_passes(Some("diesel"), "adolfo dominguez"));
    }

    /// Every pair reaching `MIN_SCORE` under brute-force `similarity` must
    /// appear in the planned candidates (unknown brands: no brand pruning).
    #[test]
    fn plan_candidates_keeps_every_above_threshold_pair() {
        let providers = [
            "Diesel Fuel For Life EDT 125 ml",
            "Adolfo Dominguez ADN Neroli Ecstasy 100 ml",
            "Kenzo Flower EDP 100 ml",
            "abcd",
            "Completely Unrelated Name XYZ",
        ];
        let products = [
            "Diesel Fuel For Life EDT",
            "Adolfo Dominguez ADN Neroli Ectasy",
            "Flower by Kenzo EDP 100 ml",
            "abce",
            "Rose Spicy EDP",
        ];
        let prepared_providers: Vec<PreparedProvider> = providers
            .iter()
            .enumerate()
            .map(|(index, name)| PreparedProvider {
                index,
                name: prepare_name(&pp_match_key(name)),
                brand: None,
            })
            .collect();
        let prepared_products: Vec<PreparedProduct> = products
            .iter()
            .enumerate()
            .map(|(index, name)| PreparedProduct {
                index,
                name: prepare_name(name),
                brand: String::new(),
            })
            .collect();
        let (candidates, stats) = plan_candidates(&prepared_providers, &prepared_products);
        let planned: std::collections::HashSet<(usize, usize)> =
            candidates.into_iter().collect();
        assert_eq!(
            stats.total_pairs,
            providers.len() * products.len()
        );
        assert_eq!(
            stats.scored + stats.prefix_skipped + stats.length_skipped,
            stats.total_pairs - stats.brand_skipped
        );
        for (i, pp) in providers.iter().enumerate() {
            for (j, product) in products.iter().enumerate() {
                if similarity(&pp_match_key(pp), product) >= MIN_SCORE {
                    assert!(
                        planned.contains(&(i, j)),
                        "lost above-threshold pair {pp:?} vs {product:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn plan_candidates_partitions_known_brands() {
        let prepared_providers = vec![
            PreparedProvider {
                index: 0,
                name: prepare_name(&pp_match_key("Diesel Fuel For Life EDT 125 ml")),
                brand: Some("diesel".to_string()),
            },
            PreparedProvider {
                index: 1,
                name: prepare_name(&pp_match_key("Diesel Fuel For Life EDT 125 ml")),
                brand: None,
            },
        ];
        let prepared_products = vec![
            PreparedProduct {
                index: 0,
                name: prepare_name("Diesel Fuel For Life EDT"),
                brand: "diesel".to_string(),
            },
            PreparedProduct {
                index: 1,
                name: prepare_name("Diesel Fuel For Life EDT"),
                brand: "adolfo dominguez".to_string(),
            },
        ];
        let (candidates, stats) = plan_candidates(&prepared_providers, &prepared_products);
        let planned: std::collections::HashSet<(usize, usize)> =
            candidates.into_iter().collect();
        // Same-brand exact pair is kept; the cross-brand twin is pruned even
        // though its name is identical (domain rule: brands never cross).
        assert!(planned.contains(&(0, 0)));
        assert!(!planned.contains(&(0, 1)));
        assert!(stats.brand_skipped >= 1);
        // An unknown-brand provider with an exact name still reaches every
        // brand group (no brand pruning without a known provider brand).
        assert!(planned.contains(&(1, 0)));
        assert!(planned.contains(&(1, 1)));
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
