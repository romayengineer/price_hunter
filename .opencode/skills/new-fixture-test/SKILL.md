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
3. `extract_products` — one product per card; price = `current_price_of(...).or_else(|| prices.last())`; name = `guess_name` (walks up from the price div: `a[title/aria-label]`, `a` text, `img alt`, else largest alphabetic text block).

## Workflow

1. **Read the fixture** and map: the grid container element (note its `class`), the repeating card element, the name element, and the price element(s). Distinguish the CURRENT selling price from regular/old/discount (e.g. `itemprop="price"`, `.price`, `.sellingPrice` vs `.regular-price`, `.discount-*`, `del`/`s`).
2. **Count cards**: `rg -c` a marker appearing once per card (e.g. `<article class="product-miniature`). That's the number of products expected.
3. **Extract expected products** with a one-off script (template below) reading name + current price. Convert prices per the parse rules.
4. **Probe the pipeline** — dump actual detection output and compare with the fixture:
   - add a temporary `tests/probe.rs` (remove before finishing):

```rust
#[test]
fn probe() {
    let html = std::fs::read_to_string("tests/fixtures/<name>.html").unwrap();
    println!("{:#?}", price_hunter::detect::detect_grid(&html));
}
```
   - run `cargo test --test probe -- --nocapture`.
5. **Fix `detect.rs` only if needed** (see failure modes). Preserve existing behavior with fallbacks; run the whole suite.
6. **Add a `detect.rs` unit test** reproducing the quirk with inline HTML (in `#[cfg(test)] mod tests`).
7. **Write `tests/<name>.rs`**, then verify: `cargo test` and `cargo clippy --all-targets`.

## Failure modes → where to fix detect.rs

- Wrong price among several on a card → prefer the current-price element. Follow the `current_price_of` pattern: scan the price div's descendants for the new signal (e.g. `itemprop="price"`, or a class containing `price` but not `regular|old|sale|was|discount|compare`), parse it, return it; the caller falls back to `prices.last()` so other sites keep working.
- Fewer products than card count → card grouping (`card_of`) or container (`best_container`) too narrow.
- Name wrong/empty → extend `find_structured_name` / `guess_name` for the new markup.
- Wrong container class → `best_container` picked another div (density vs count tradeoff).
- Wrong number value → `parse_price` / `number_tokens` / `split_decimal` (thousands vs decimal).
- Discounts / list-prices leaking in → ensure discount/old elements are never chosen.

## Price parsing rules (parse_price)

- `$242.100` → 242100.0 (dot + 3 digits = thousands separator)
- `12,99` → 12.99 (comma + 2 digits = decimal)
- `1.234,56` → 1234.56
- Bare integers: single token in an otherwise numeric div → parsed as-is

## Extraction script template (adapt selectors to the fixture)

```python
import re, html as h
src = open('tests/fixtures/<name>.html').read()
for b in re.split(r'<article ', src)[1:]:   # split on one-per-card marker
    name = re.search(r'<h2[^>]*>\s*<a[^>]*>([\s\S]*?)</a>', b)
    price = re.search(r'itemprop="price"[\s\S]*?<span>\$?([\d.,]+)</span>', b)
    print(f'{re.sub("\\s+", " ", h.unescape(name.group(1))).strip()} | {price.group(1)}')
```

## Gotchas

- Expected `price_text` is always `String::new()`.
- Pass ONE container class that's present on the container element.
- PrestaShop: current price = `span[itemprop="price"]`; `.regular-price` and `.discount-*` must be ignored.
- Card count from `rg -c` must match the extracted product count.
- Don't run `--ignored`/live tests. Don't commit unless asked.
