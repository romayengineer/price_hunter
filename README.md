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
persisted to a [PocketBase](https://pocketbase.io) instance **through its HTTP
API only** — the scraper never writes SQL and never touches the database file
directly.

### PocketBase

Price Hunter persists every capture to PocketBase — a self-hosted,
single-executable backend with an admin dashboard and a Record API. PocketBase
is not required to run the scraper (it always writes JSON); it's the retrieval
layer for the app.

The PocketBase data directory lives **outside the repo** at
`~/.local/share/price_hunter/pb_data` (override with `POCKETBASE_DATA_DIR`), so
a fresh clone or an accidental project-folder delete never loses the database.

Setup:

```sh
POCKETBASE_SUPERUSER_PASSWORD='<password>' \
  bash pocketbase/scripts/setup_pocketbase.sh   # creates superuser + data dir

# Start the server (migrations apply on first start, creating the collections):
bash pocketbase/scripts/run_pocketbase.sh
```

- Admin dashboard (browse captures/products): `http://127.0.0.1:8090/_/`
  — log in with the superuser (`admin@pricehunter.local` by default).
- Captures API: `http://127.0.0.1:8090/api/collections/captures/records`
- Products API: `http://127.0.0.1:8090/api/collections/products/records`

The scraper authenticates as the superuser (env: `POCKETBASE_URL`,
`POCKETBASE_SUPERUSER_EMAIL`, `POCKETBASE_SUPERUSER_PASSWORD`) and writes
through the Record API; the collections are world-readable so other apps can
query prices without a token. The schema lives in `pocketbase/migrations/`
(JS migrations — no SQL) and is applied automatically on `serve`.

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

The same data lands in the `captures` and `products` PocketBase collections
(url, host, captured_at, container metadata, and one product record per product
with name/price, linked to its capture).

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

The store live test requires a running PocketBase with the collections created
(it round-trips through the API only — no SQL):

```sh
# Start PocketBase first, then:
POCKETBASE_SUPERUSER_PASSWORD='<password>' \
  cargo test --test store_live -- --ignored
```

Set `PRICE_HUNTER_DUMP_HTML=1` when running the compreahora live test to save
the rendered page to `captures/diagnostic/` for debugging.

**Note:** Chrome locks `profiles/chrome` while a `cargo run` browser is open —
close it before running the tests.
