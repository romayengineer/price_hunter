---
name: new-fixture-test
description: Use when creating a fixture-based integration test in price_hunter from a new saved HTML page, or when an existing fixture test fails to extract all products. Extracts products into a Vec<Product>, wires common::assert_fixture, and improves detect.rs when the markup isn't handled.
---

# New fixture test (price_hunter)

Turn a saved HTML fixture into a green integration test, fixing `detect.rs` when the pipeline can't extract the products.

## Conventions

- Fixtures: `tests/fixtures/<name>.html` (saved grid fragment; scraper auto-wraps html/body).
- Tests: `tests/<name>.rs`, one file per site. Shape:

```rust
mod common;
use price_hunter::detect::Product;

fn products() -> Vec<Product> {
    vec![
        Product { name: String::from("..."), price_text: String::new(), price: 1234.0 },
        // ... one per card, in fixture order
    ]
}

#[test]
fn extracts_all_products_from_<name>_fixture() {
    common::assert_fixture("tests/fixtures/<name>.html", &products(), "<container-class>");
}
```

- Expectations use `price_text: String::new()` — `common::products_found` compares only `name` + `price`.
- `common::assert_fixture` checks every expected product exists (name+price) and the container has the given class (membership).
- Fixture tests need no browser/network. Run `cargo test`; skip `--ignored` live tests.

## Detection pipeline (src/detect.rs)

`detect_grid(source)`:
1. `find_price_divs` — divs whose own text classifies as prices (`classify_div`); plus a separate path for spans whose class contains `product-price`.
2. `best_container` — div with ≥2 price divs, maximizing price count then density.
3. `extract_products` — one product per card; price = `current_price_of(...).or_else(|| prices.last())`; name = `guess_name` (walks up from the price div: `a[data-role="product-item-name"]`, `a[title/aria-label]`, `a` text, `img alt`, else largest alphabetic text block).

`current_price_of` recognizes current-price elements by `itemprop="price"` or `data-price-type="finalPrice"` (Magento), falling back to `prices.last()`.

`card_of` groups each price div under the node whose parent is the container; if that node is a list/table wrapper (`ul`/`ol`/`tbody`/`table` or `role="list"`) it returns the element one level deeper (e.g. the `li` card), so cards inside a `<ul>` still split correctly.

## Workflow

1. **Read the fixture** and map: the grid container element (note its `class`), the repeating card element, the name element, and the price element(s). Distinguish the CURRENT selling price from regular/old/discount (e.g. `itemprop="price"`, `.price`, `.sellingPrice` vs `.regular-price`, `.discount-*`, `del`/`s`).
2. **Count cards**: `rg -c` a marker appearing once per card (e.g. `<article class="product-miniature`). That's the number of products expected.
3. **Extract expected products** with a one-off script (template below) reading name + current price. Convert prices per the parse rules.
4. **Probe the pipeline** — dump actual detection output and compare with the fixture. There are permanent, env-var-parameterized diagnostic tests (no temp files to create/delete):
   - `PRICE_HUNTER_PROBE_FIXTURE=tests/fixtures/<name>.html cargo test --test probe -- --nocapture` — prints the full `Detection` (container + products).
   - `PRICE_HUNTER_MEASURE_FIXTURE=tests/fixtures/<name>.html cargo test --test measure -- --nocapture` — prints every candidate container ranked by density with `p`/`d`/`density` and marks which one `detect_grid` selects. Use this when the wrong container is picked.
   - All three are no-ops without their env var, so normal test runs stay clean.
5. **Fix `detect.rs` only if needed** (see failure modes). Preserve existing behavior with fallbacks; run the whole suite.
6. **Add a `detect.rs` unit test** reproducing the quirk with inline HTML (in `#[cfg(test)] mod tests`).
7. **Write `tests/<name>.rs`**, then verify: `cargo test` and `cargo clippy --all-targets`.

## Diagnosing the live site

When the fixture passes but the real site detects wrong (e.g. only 2 products, wrong container, prices `0.3`/`1.5`), the page differs from the fixture — usually there are widgets/carousels above the grid and a huge `page-wrapper`. Use:

- `PRICE_HUNTER_LIVE_URL=<url> PRICE_HUNTER_DUMP_HTML=1 cargo test --test live_probe -- --ignored -- --nocapture` — opens Chrome (persistent profile), navigates, scrolls, dumps the rendered HTML to `captures/diagnostic/live-probe.html`, and prints the detected container + products. Then run `probe`/`measure` against the dumped file.
- Compare the live markup with the fixture: the grid should be `div.products.wrapper` etc. If `best_container` picks the whole-page wrapper, the density scoring is off (see failure modes).

## Failure modes → where to fix detect.rs

- Wrong price among several on a card → prefer the current-price element. Follow the `current_price_of` pattern: scan the price div's descendants for the new signal (e.g. `itemprop="price"`, `data-price-type="finalPrice"`, or a class containing `price` but not `regular|old|sale|was|discount|compare|base`), parse it, return it; the caller falls back to `prices.last()` so other sites keep working.
- Fewer products than card count (often 1 collapsed product) → `card_of` hit a shared wrapper (`ul`/`ol`/`tbody`/`table` or `role="list"`) instead of the per-product card; extend `is_card_list_wrapper` or `card_of`.
- Name wrong/empty (e.g. a brand or breadcrumb `<a>` beats the product name) → extend `find_structured_name` / `guess_name` for the new markup (e.g. `a[data-role="product-item-name"]`).
- Wrong container class → `best_container` picked another div (density vs count tradeoff).
- Wrong number value → `parse_price` / `number_tokens` / `split_decimal` (thousands vs decimal).
- Discounts / list-prices leaking in → ensure discount/old/base elements are never chosen.

## Price parsing rules (parse_price)

- `$242.100` → 242100.0 (dot + 3 digits = thousands separator)
- `12,99` → 12.99 (comma + 2 digits = decimal)
- `1.234,56` → 1234.56
- Bare integers: single token in an otherwise numeric div → parsed as-is

## Extraction script template (adapt selectors to the fixture)

PrestaShop (`itemprop="price"`):

```python
import re, html as h
src = open('tests/fixtures/<name>.html').read()
for b in re.split(r'<article ', src)[1:]:   # split on one-per-card marker
    name = re.search(r'<h2[^>]*>\s*<a[^>]*>([\s\S]*?)</a>', b)
    price = re.search(r'itemprop="price"[\s\S]*?<span>\$?([\d.,]+)</span>', b)
    print(f'{re.sub("\\s+", " ", h.unescape(name.group(1))).strip()} | {price.group(1)}')
```

Magento / Hyva (`data-price-type="finalPrice"`):

```python
import re, html as h
src = open('tests/fixtures/<name>.html').read()
for b in re.split(r'<li class="flex flex-col">', src)[1:]:   # split on one-per-card marker
    name = re.search(r'data-role="product-item-name"[^>]*>\s*([\s\S]*?)</a>', b)
    price = re.search(r'data-price-type="finalPrice"[\s\S]*?<span\s+class="price">\$[^\d]*([\d.,]+)</span>', b)
    print(f'{re.sub("\\s+", " ", h.unescape(name.group(1))).strip()} | {price.group(1)}')
```

## Gotchas

- Expected `price_text` is always `String::new()`.
- Pass ONE container class that's present on the container element.
- Keep the diagnostic tests, don't delete them:
  - `tests/probe.rs` — fixture dumper (`PRICE_HUNTER_PROBE_FIXTURE=...`).
  - `tests/measure.rs` — container ranking (`PRICE_HUNTER_MEASURE_FIXTURE=...`).
  - `tests/live_probe.rs` — live browser probe (`PRICE_HUNTER_LIVE_URL=...`, `PRICE_HUNTER_DUMP_HTML=1`; runs with `-- --ignored`).
- PrestaShop: current price = `span[itemprop="price"]`; `.regular-price` and `.discount-*` must be ignored.
- Magento/Hyva: current price = `[data-price-type="finalPrice"]`; ignore `oldPrice`/`basePrice` and `.product-installments` amounts.
- Card count from `rg -c` must match the extracted product count.
- Don't run `--ignored`/live tests. Don't commit unless asked.
