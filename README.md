# Price Hunter

Price Hunter is a Rust tool that opens a real, user-controlled Chrome window and,
in the background, detects product names and prices from the current page into
JSON captures. You keep browsing normally; it watches and records.

It uses `thirtyfour` (Selenium WebDriver for Rust) and a heuristic detector that
works across differently-structured e-commerce pages without site-specific code.

## How it works

- Chrome launches with a **persistent profile** in `profiles/chrome/`
  (gitignored), so logins, cookies, and session state survive between runs.
  Log in once; later runs reuse the session.
- The matching `chromedriver` is auto-downloaded on first run and shut down on
  exit — nothing to install.
- While the browser is open, a poll loop reads the live page HTML
  (`driver.source()`), runs the grid detector, and writes a capture whenever the
  detected products change.
- Detection is heuristic: it finds the product grid, extracts each product's
  name, and reconstructs prices. It handles fragmented prices split across
  `product-price` spans (VTEX), old/current price pairs, and falls back to
  text-based detection on other layouts.

## Requirements

- Rust `stable` (pinned via `rust-toolchain.toml`).
- No chromedriver installation needed.

## Usage

```sh
# Open Chrome and keep it open until you close the window or press Ctrl+C.
cargo run

# Optionally start on a specific page.
cargo run -- https://example.com
```

If navigation fails, the session stays open — just type the address in the
browser yourself.

### Examples

```sh
cargo run -- https://www.compreahora.com.ar/categoria/perfumeria
cargo run -- https://perfumeriasfabilu.com.ar/categoria/perfumeria
cargo run -- https://www.beauty24.com.ar/perfumes-y-fragancias
```

## Output

Captures are written to `captures/<domain>/capture-<timestamp>.json`, organized
by site hostname. A new timestamped file is written whenever the detected
products (prices or names) change on a later poll. The same captures are also
persisted to the SQLite database TrailBase serves (`traildepot/data/main.db`,
override with `PRICE_HUNTER_DB`).

### TrailBase

Price Hunter persists every capture to a SQLite database exposed through
[TrailBase](https://trailbase.io) — a self-hosted, single-executable backend
with an admin dashboard and type-safe Record APIs. TrailBase is not required to
run the scraper (it always writes JSON); it's the retrieval layer for the app.

Setup:

```sh
bash trailbase/scripts/setup_trailbase.sh   # installs `trail`, seeds traildepot/
trail run                                    # serves http://localhost:4000
```

- Admin dashboard (browse captures/products): `http://localhost:4000/_/admin/`
  — credentials are printed on the first `trail run`.
- Captures API: `http://localhost:4000/api/records/v1/captures`
- Products API: `http://localhost:4000/api/records/v1/products`

The scraper writes directly to `traildepot/data/main.db` (the same file
TrailBase serves), so no app user is needed — the Record APIs are world-readable
and any writes go straight to the database file.

The schema lives in `trailbase/migrations/` and the Record API config in
`trailbase/config.textproto`. The runtime depot (`traildepot/`) is gitignored;
re-run the setup script on a fresh checkout to recreate it.

```json
{
  "url": "https://www.beauty24.com.ar/perfumes-y-fragancias",
  "captured_at": 1785802653,
  "container": {
    "classes": ["vtex-search-result-3-x-gallery"],
    "id": null,
    "child_count": 5
  },
  "detected_cards": 5,
  "products": [
    { "name": "Dylan Blush Pink EDP 100 ml + Neceser", "price_text": "328.000", "price": 328000.0 }
  ]
}
```

The same data lands in the `captures` and `products` SQLite tables (host,
captured_at, container metadata, and one row per product with name/price).

## Tests

```sh
# Fixture-based detection tests (no network or browser needed).
cargo test
```

Live integration tests are `#[ignore]`d and require network access (and, for
compreahora, a logged-in session in `profiles/chrome`):

```sh
cargo test --test compreahora_live -- --ignored
cargo test --test fabilu -- --ignored
cargo test --test beauty24 -- --ignored
```

Set `PRICE_HUNTER_DUMP_HTML=1` when running the compreahora live test to save
the rendered page to `captures/diagnostic/` for debugging.

**Note:** Chrome locks `profiles/chrome` while a `cargo run` browser is open —
close it before running the tests.
