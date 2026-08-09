# AGENTS.md

## Project
Price Hunter — a Rust library + binary that detects product price grids in
arbitrary e-commerce HTML and captures them as JSON. Uses `thirtyfour`
(Selenium WebDriver for Rust) and `scraper` for offline HTML parsing.

- `src/detect.rs` — pure HTML → `Detection` pipeline (no browser needed; the
  heart of the project).
- `src/browser.rs` — launches Chrome via a persistent profile.
- `src/capture.rs` — writes JSON captures under `captures/<host>/`.
- `src/store.rs` — persists captures + products to a running PocketBase via its
  Record API using the `pocketbase-sdk` crate (async HTTP, no SQL, no DB file
  access). Mirrors the `captures`/`products` collections in
  `pocketbase/migrations/`.
- `src/main.rs` — `cargo run`: opens a real, user-controlled browser and polls
  it for captures in the background.
- `pocketbase/` — PocketBase migrations (JS, no SQL) + `scripts/setup_pocketbase.sh`
  for the retrieval/app layer (admin dashboard + Record APIs). Not required to
  run the scraper.
- `tests/` — fixture-based integration tests (offline), live browser tests
  (`#[ignore]`), and diagnostic tools.

## Architecture intent
- Opens a real browser that remains interactive and user-controlled.
- In the background, the poll loop fetches the live page HTML (via
  `driver.source()`) and runs it through `detect::detect_grid`, extracting
  product names and prices into a JSON capture.
- Target users: people scraping prices from pages they already know while
  continuing to use the browser normally.

## Detection pipeline (src/detect.rs)
`detect_grid(source) -> Option<Detection>`:
1. `find_price_divs` — find divs whose own text classifies as prices
   (`classify_div`). Two paths: generic div own-text, and a dedicated path for
   spans whose class contains `product-price`.
2. `best_container` — rank candidate grid containers. Among the most
   price-rich divs (top half by price count), picks the **densest**
   (`price_count / div_count`). This is what separates the real grid from the
   whole-page `page-wrapper` on live sites.
3. `extract_products` — one `Product` per card (name + current price). Price
   prefers `current_price_of(...)` (an element marked `itemprop="price"` or
   `data-price-type="finalPrice"`) and falls back to the last detected price;
   name comes from `guess_name` (walks up from the price div:
   `a[data-role="product-item-name"]`, `a[title/aria-label]`, `a` text,
   `img alt`, else the largest alphabetic text block).

Supporting heuristics worth knowing:
- `card_of` groups price divs into per-product cards; it descends one level
  through list/table wrappers (`ul`/`ol`/`tbody`/`table` or `role="list"`) so
  cards inside a `<ul><li>` grid still split correctly.
- `current_price_of` recognizes `itemprop="price"` (PrestaShop) and
  `data-price-type="finalPrice"` (Magento/Hyva), ignoring `regular-price`,
  `oldPrice`, `basePrice`, and installment/discount amounts.
- `parse_price`/`number_tokens`/`split_decimal` handle `242.100` (thousands),
  `12,99` (decimal), `1.234,56`, and bare integers.
- `diagnose_containers(source) -> Vec<ContainerCandidate>` is a public debug
  helper that ranks every candidate container with `price_count`, `div_count`,
  `density`, and a `selected` flag. Use it when the wrong container is picked.

Public API (`src/lib.rs` re-exports modules): `browser::{launch, profile_dir}`,
`capture::write_capture`, `detect::{detect_grid, diagnose_containers,
Price, Product, Container, Detection, ContainerCandidate}`, `store::{Store}`.
`Store` is a sync client (`Store::connect()` then `save(url, captured_at,
&Detection)`) that talks to PocketBase over its Record API; it returns `None`
from `main`'s `connect_store()` if PocketBase is down, leaving JSON-only mode.

## Toolchain
- Pinned to `stable` via `rust-toolchain.toml` (rustup auto-uses it).
- `cargo`/`rustc`/`rustup` work on PATH: `~/.cargo/bin` proxies chain to the
  real Homebrew rustup binary (`/usr/local/Cellar/rustup/<ver>/libexec/bin/rustup`).
  No PATH export needed. (The direct toolchain binaries also remain at
  `~/.rustup/toolchains/stable-x86_64-apple-darwin/bin`.)
- No chromedriver needs installing: `WebDriver::managed` auto-downloads the
  matching chromedriver on first run and shuts it down on exit.
- Sessions persist across runs: Chrome launches with a dedicated profile in
  `profiles/chrome/`, so logins/cookies survive between executions. Log in once;
  later `cargo run`s reuse the session.

## Run
- `cargo run` opens Chrome and keeps it open until the window is closed or Ctrl+C.
- Optional URL arg: `cargo run -- https://example.com`. Failed navigation warns
  but keeps the session alive.
- Captures are automatic: as soon as a grid is detected on the current page the
  products are written to `captures/<domain>/capture-<timestamp>.json` (organized
  by site hostname), and a new timestamped file is written again whenever the
  detected products (prices or names) change on a later poll. Each capture is
  also persisted to PocketBase through its HTTP API only (env:
  `POCKETBASE_URL`, `POCKETBASE_SUPERUSER_EMAIL`,
  `POCKETBASE_SUPERUSER_PASSWORD`), which serves the Record APIs. Store failures
  are logged and never crash the browser session.

## Tests
- `cargo test` runs the fixture-based detection tests (no network/browser needed)
  plus the lib unit tests. Always run `cargo test` AND `cargo clippy --all-targets`
  after changing code.
- Fixture tests live in `tests/<site>.rs` and share `tests/common/mod.rs`
  (`assert_fixture(path, &products(), container_class)`, which checks every
  expected `Product` (name + price) exists and the container has the class).
  Expected products use `price_text: String::new()` — only name and price are
  compared. Fixtures live in `tests/fixtures/<site>.html`.
- Live integration tests are `#[ignore]`d and require network plus a real session:
  - `cargo test --test compreahora_live -- --ignored` opens Chrome using the
    persistent `profiles/chrome` session, visits the perfumeria category, and
    asserts names + prices are extracted. Set `PRICE_HUNTER_DUMP_HTML=1` to save
    the rendered page to `captures/diagnostic/` for debugging.
  - `cargo test --test fabilu -- --ignored` fetches fabilu over plain HTTP.
  - `cargo test --test parfumerie_live -- --ignored` opens Chrome, visits the
    `/fragancias` category, scrolls to trigger infinite scroll, and asserts at
    least 10 products with names + prices.
  - `cargo test --test store_live -- --ignored` requires a running PocketBase
    (see `pocketbase/scripts/setup_pocketbase.sh`) and round-trips a capture +
    products through the Record API only — verifies the store works without any
    SQL or DB-file access.
- Diagnostic tools (keep these; they are no-ops without their env var):
  - `PRICE_HUNTER_PROBE_FIXTURE=<path> cargo test --test probe -- --nocapture`
    dumps the full `Detection` (container + products) for any HTML file.
  - `PRICE_HUNTER_MEASURE_FIXTURE=<path> cargo test --test measure -- --nocapture`
    ranks every candidate container (p/d/density + selected flag) — use when the
    wrong container is picked.
  - `PRICE_HUNTER_LIVE_URL=<url> PRICE_HUNTER_DUMP_HTML=1 cargo test --test live_probe -- --ignored -- --nocapture`
    opens Chrome, navigates, scrolls, optionally dumps the rendered HTML, and
    prints the detected container + products.
- Close any `cargo run` browser first — Chrome locks `profiles/chrome` while running.

## Adding a new site / fixture
There is a skill and a command for this; use them instead of improvising:
- Skill: `.opencode/skills/new-fixture-test/SKILL.md` (auto-loaded by the model
  when the task matches; follow its workflow).
- Command: `/new-fixture-test tests/fixtures/<site>.html` runs the whole flow:
  map the fixture, count cards, extract expected products, probe the pipeline,
  fix `detect.rs` only at the gap (with a fallback + unit test), write
  `tests/<site>.rs`, then verify `cargo test` + `cargo clippy --all-targets`.

Workflow summary (details in the skill):
1. Save the site's rendered grid as `tests/fixtures/<site>.html`.
2. Identify container, card, name, and current-price markup; count cards with
   `rg -c`.
3. Extract expected products (one-off python script; templates in the skill for
   PrestaShop and Magento/Hyva markup).
4. Probe with `probe.rs`; if detection differs from the fixture, use
   `measure.rs` / `live_probe.rs` to diagnose and fix `detect.rs` with a
   backward-compatible fallback.
5. Write the fixture test and a `detect.rs` unit test for any new markup quirk.

Known markup classes the detector already handles: PrestaShop
(`span[itemprop="price"]` in `article.product-miniature`), Magento/Hyva
(`data-price-type="finalPrice"` in `ul > li` cards), WooCommerce
(`woocommerce-Price-amount`), VTEX (`sellingPrice`), generic div grids.

## Gotchas
- `captures/` is gitignored — write capture output there; do not expect it committed.
- Known upstream bugs are tracked in `BUGS.md` — check it before debugging
  backend tooling issues.
- PocketBase data lives OUTSIDE the repo at
  `~/.local/share/price_hunter/pb_data` (override with `POCKETBASE_DATA_DIR`) so
  a fresh clone or an accidental project-folder delete never loses the
  database. The checked-in source of truth is `pocketbase/migrations/` (JS, no
  SQL) + `pocketbase/scripts/setup_pocketbase.sh`. Re-run the setup script on a
  fresh checkout.
- **Never write SQL and never touch the database file directly** — the scraper
  persists ONLY through the PocketBase Record API (authenticated as superuser).
  Schema changes go in a NEW `pocketbase/migrations/<ts>_<name>.js` file
  (applied automatically on the next `pocketbase serve`); do not use `sqlite3`
  on the PocketBase data file.
- Do not `driver.quit()`/drop early while the user is driving the browser; keep
  the `WebDriver` alive until the browser window closes (probe with
  `driver.current_url()`).
- If a browser-based test "passes" instantly with no output, a leftover Chrome
  is probably holding `profiles/chrome` — kill it (`pkill -f profiles/chrome`)
  and rerun.
- `.opencode/node_modules` is gitignored; don't touch or commit it.
- Keep changes backward-compatible: when extending `detect.rs`, preserve the
  existing fallbacks (`current_price_of` → `prices.last()`, `card_of` descent,
  etc.) so all existing fixture tests keep passing.
