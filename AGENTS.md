# AGENTS.md

## Project
Price Hunter — a Rust library + binary that detects product price grids in
arbitrary e-commerce HTML and captures them as JSON. Uses `thirtyfour`
(Selenium WebDriver for Rust) and `scraper` for offline HTML parsing.

- `src/detect.rs` — pure HTML → `Detection` pipeline (no browser needed; the
  heart of the project).
- `src/browser.rs` — launches Chrome via a persistent profile. `launch()`
  opens a visible window; `launch_with(headless: bool)` adds
  `--headless=new` for headless runs (both use the same persistent profile).
- `src/capture.rs` — writes JSON captures under `captures/<host>/`.
- `src/autoscrape.rs` — automatic site scraping. The `AutoScraper` trait
  abstracts how a listing page reveals more products (site-specific); concrete
  strategies are `ScrollAndClick` (scroll down and click a load-more button,
  explicit CSS selector or heuristic), `InfiniteScroll` (scroll to bottom; the
  count-based termination decides the end) and `PageParam` (navigate
  `?page=N` until a page has no grid). `scrape_until_no_growth` drives any
  strategy: it re-detects the product count after every step and stops when the
  count stops increasing for `NO_GROWTH_LIMIT` rounds (or the strategy reports
  exhaustion / `MAX_STEPS` is hit). `strategy_for(url, &AutoScrapeOptions)`
  picks the strategy from a host registry (`default_strategy`) with CLI
  overrides.
- `src/store.rs` — persists detections to a running PocketBase via its Record
  API using the `pocketbase-sdk` crate (async HTTP, no SQL, no DB file access).
  Writes the normalized schema (`providers`, `scrapes`, `provider_products`,
  `provider_product_images`, `provider_product_prices`) defined in `DATABASE.md` and mirrored in
  `pocketbase/migrations/`.
- `src/main.rs` — `cargo run`: opens a real, user-controlled browser and polls
  it for captures in the background.
- `src/instance.rs` — single-instance lock via a PID file at
  `~/.config/price_hunter/price_hunter.pid` (honors `$XDG_CONFIG_HOME`). A new
  `cargo run` kills the previous running instance (and any orphaned
  Chrome/chromedriver holding `profiles/chrome`) before launching its browser;
  the file is removed on graceful exit.
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
3. `extract_products` — one `Product` per card (name + current price, plus the
   card link `url`, deduped `images`, and best-effort `currency`). Price
   prefers `current_price_of(...)` (an element marked `itemprop="price"` or
   `data-price-type="finalPrice"`) and falls back to the last detected price;
   name comes from `guess_name` (walks up from the price div:
   `a[data-role="product-item-name"]`, `a[title/aria-label]`, `a` text,
   `img alt`, else the largest alphabetic text block). The name is then
   post-processed by `enrich_name_with_size`: if it lacks a size (number +
   unit), the size is taken from the card's SKU selector (the `--selected`
   option first), then from the product URL slug, then by appending `ml` to a
   trailing bare number (e.g. `edp 50` → `edp 50 ml`). Names that already
   carry a size (`100 ml`, `X50ML`, `132 g`, …) are left unchanged. Finally
   `enrich_name_with_brand` prepends the brand the site renders as a separate
   card element — VTEX `productBrandName`/`productBrandContainer`, Magento/Hyva
   `<strong class="product brand">`/`product-item-brand`, and `__brand`-suffixed
   headings (e.g. compreahora) — unless every brand token is already present in
   the name (so "…Dove…" and "Adidas …" aren't duplicated, and sites without
   brand markup like todoslosperfumes stay unchanged).

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
`capture::write_capture`, `config::Config`, `detect::{detect_grid,
diagnose_containers, Price, Product, Container, Detection, ContainerCandidate}`,
`store::{Store}`. `Store` is a sync client (`Store::connect()` then `save(url,
captured_at, capture_path, &Detection)`) that talks to PocketBase over its Record
API. The connection is **required**: `main` fails fast (`exit 1`, before opening
Chrome) if PocketBase is unreachable or no password is configured — there is no
JSON-only fallback.

## Configuration (`src/config.rs`)
Runtime settings live in a TOML file in the XDG config dir:
`$XDG_CONFIG_HOME/price_hunter/config.toml` (default
`~/.config/price_hunter/config.toml`).

```toml
[pocketbase]
url = "http://127.0.0.1:8090"
email = "admin@pricehunter.local"
password = "change-me"          # required; first run writes a commented template
```

- Precedence: `POCKETBASE_URL` / `POCKETBASE_SUPERUSER_EMAIL` /
  `POCKETBASE_SUPERUSER_PASSWORD` env vars **override** the file, which
  overrides the built-in defaults.
- `Config::ensure_template()` writes a commented template on first `cargo run`
  (mode 600) and `setup_pocketbase.sh` writes the real file after it creates
  the superuser. Treat the file as a secret: it holds the superuser password.
- `Config::load()` returns defaults for a missing file and errors on a malformed
  one. Keep the `[pocketbase]` section name stable — `Store::connect()` depends
  on it.
- `POCKETBASE_DATA_DIR` is **not** in config.toml: the shell scripts need it
  before Rust runs, so it stays an env var.

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
- `bash pocketbase/scripts/run_pocketbase.sh` starts the PocketBase server
  (data dir outside the repo, repo-relative migrations dir).
- Any command can be made non-interactive by adding `-yes` (or `-y`): it
  auto-accepts confirmation prompts instead of reading stdin. Currently only
  `-delete-unbranded` prompts, so e.g. `cargo run -- -delete-unbranded -yes`
  deletes every page without asking. The flag is a modifier — it never selects
  a command by itself.
- `cargo run` opens Chrome and keeps it open until the window is closed or Ctrl+C.
  It connects to PocketBase **first** and exits with an error if it can't. Run
  `bash pocketbase/scripts/setup_pocketbase.sh` once (writes
  `~/.config/price_hunter/config.toml`), or edit that file / export
  `POCKETBASE_SUPERUSER_PASSWORD` manually.
- Optional URL arg: `cargo run -- https://example.com`. Failed navigation warns
  but keeps the session alive.
- Captures are automatic: as soon as a grid is detected on the current page the
  products are written to `captures/<domain>/capture-<timestamp>.json` (organized
  by site hostname), and a new timestamped file is written again whenever the
  detected products (prices or names) change on a later poll. Each capture is
  also persisted to PocketBase through its HTTP API only (env:
  `POCKETBASE_URL`, `POCKETBASE_SUPERUSER_EMAIL`,
  `POCKETBASE_SUPERUSER_PASSWORD`), which serves the Record APIs. A failed
  *save* is logged and never crashes the browser session — but the startup
  *connection* is mandatory (`main` exits if it cannot connect).
- `cargo run -- -import-brands <file.csv>` imports the canonical brand list
  (one column, brand name) into the `brand` collection. A leading `name`
  header row, empty rows and duplicates are skipped; the unique index on
  `name` backstops idempotency. The collection is created by migration
  `1787000002_brand.js` and is intended for later use in flagging
  provider_products whose names contain no known brand.
- `cargo run -- -import-unmatched` proposes canonical `products` rows from
  unmatched provider products (`product_id` empty). Each name is split into
  brand (guessed from the `brand` table, all brand tokens must appear),
  product_name, and size (trailing number normalized to `N ml`, other units
  like `g`/`l` preserved). Proposals that already exist in `products` (same
  full name) or duplicate an earlier proposal are dropped, and the rest are
  inserted **one at a time** after a single-key `(y/N)` prompt — `y` inserts,
  anything else skips (pressing just `y`, no Enter needed, via raw-mode
  `termios` reads). The insert respects the `(brand, product_name, size)`
  unique index, so an `AlreadyExists` is reported instead of a 400. Add `-yes`
  to insert every proposal without prompting.
- `cargo run -- -match-products` scores every (provider product × canonical
  product) comparison and stores it in `provider_product_matches` **only when
  the score ≥ `MIN_SCORE` (0.6)** — weaker pairs are scored but never written,
  so they are recomputed on a later run instead of accumulating in the table.
  Comparisons already stored on a previous run are skipped, so re-runs only
  compute new pairs. Each qualifying score is written **immediately after it is
  computed** (one insert per HTTP request, via a pooled `ureq::Agent` so bulk
  inserts reuse one TCP connection instead of exhausting macOS ephemeral
  ports), so a crash loses no completed scores. The existing comparisons are
  loaded per provider product with indexed filter queries (a full-table OFFSET
  scan is ~140 ms/page and scales with the stored subset). A
  `Progress: X.XX%` line is redrawn in place during the backfill. A pair that
  already exists (unique index — e.g. a concurrent run inserted it) is reported
  as already computed instead of aborting with a 400. After the backfill,
  provider products are linked using the stored scores ≥ `MIN_SCORE`. Note:
  existing rows below 0.6 (written by older versions that cached every score)
  are left in place — they count as already-computed skips but are never linked.
  The `score` field was made non-required by migration
  `1787000001_allow_zero_scores.js` — PocketBase treats `required` numbers as
  blank when they are 0; the migration is now moot but kept for history.
- `cargo run -- -link-matches` re-links provider products to canonical products
  using **only the already-stored comparisons** (queries just the `score >=
  MIN_SCORE` subset — no backfill, completes in seconds). Use it to refresh
  links after scraping new data when you don't want to wait for a full
  `-match-products` pass. This is what keeps the matrix/CSV populated; a full
  run interrupted during the long backfill leaves the old links stale and the
  matrix empty.
- `cargo run -- -match-brands` assigns a brand to every provider product and
  writes it to `provider_products.brand_id` (only when it changes). A provider
  product linked to a canonical product takes that product's `brand`; the rest
  are fuzzy-matched against the `brand` table by token coverage
  (`matching::brand_coverage`, all brand tokens must appear — Sørensen-Dice
  scores too low when a brand is a small slice of a long name). Unresolved
  products keep `brand_id` empty so `brand_id=null` finds them. Note:
  PocketBase serializes unset relations as `""`, not `null`, so `Some("")` is
  treated as "unset" throughout. The `brand_id` field is added by migration
  `1787000003_provider_product_brand.js`.
- `cargo run -- -report-missing-brands` lists every provider product linked to a
  canonical product (`product_id` set) whose stored name does not contain that
  product's brand (token coverage < 1.0). These are candidate extractor bugs:
  the provider site renders the brand in the card, so the scraped name should
  carry it. Prints `<provider_domain>\t<name>\t<brand>\t<product_id>\t<provider_product_id>`
  per affected row and exits.
- `cargo run -- -delete-unbranded` lists every provider product whose name
  contains no known brand (brand table + `products.brand`, all brand tokens
  must be present) and deletes them in pages of 50, asking for confirmation
  before each page (`y`/`yes` deletes the page and continues; anything else
  aborts). Deleting a row also removes its `provider_product_prices`,
  `provider_product_images` and `provider_product_matches` rows (PocketBase has
  no cascade deletes). The lookups and deletes ride the pooled `ureq` agent (one
  TCP connection) — the SDK's one-shot client would exhaust macOS ephemeral
  ports over a page of 50 products. Use it to clean up stale rows scraped before the
  extractor included brand names. Note: sites whose cards render no brand (e.g.
  todoslosperfumes) will have every row flagged — the per-page prompt is the
  safety net.
- `cargo run -- -matrix-server` serves the product × provider price matrix for
  the local Flutter UI. Binds to `127.0.0.1:8091` (override
  `PRICE_HUNTER_MATRIX_PORT`) and rebuilds the matrix from PocketBase on every
  request. `GET /matrix` returns
  `{generated_at, providers:[{id, domain, name}], rows:[{product_id, name,
  prices:{<provider_id>: price}}]}` — one row per product with at least one
  linked provider product (no all-blank rows), latest price per provider
  (lowest when a product maps to several listings on one provider). The
  blocking PocketBase queries run via `spawn_blocking` (the SDK is ureq-based,
  not async). Requires PocketBase to be up; it uses the same config as
  `cargo run`.
- `cargo run -- -export-matrix <file.csv>` writes the same matrix to a CSV
  file (one column per provider — header = domain — one row per product, raw
  numeric prices, blank cell when a provider doesn't carry the product,
  UTF-8 BOM so Excel detects the encoding). Built from `Store::matrix()`, so
  rows are the same as `GET /matrix` (products priced at ≥2 providers).
- `cargo run -- -export-products <file.csv>` writes the canonical `products`
  table to a CSV with `brand,product_name,size` columns (one row per product,
  sorted by display name, UTF-8 BOM for Excel). Exports every product
  including inactive ones. Note: the header row means the file is not a
  drop-in `-import-products` input (the importer does not skip headers).
- `cargo run -- -export-brands <file.csv>` writes the canonical `brand` table
  to a CSV with a single `name` column (one row per brand, sorted by name,
  UTF-8 BOM for Excel). Exports every brand. The `name` header matches the
  table column, so the file round-trips through `-import-brands`.
- `cargo run -- -auto-scrape <url>` automatically scrapes a listing page to
  completion and persists the largest grid detected (JSON capture +
  PocketBase, same as the browse mode). It navigates to `url`, then drives the
  site-specific strategy until the detected product count stops increasing:
  - default (and unknown hosts): scroll down and click a load-more button
    (`-button <css>` forces the selector; otherwise common load-more
    classes/aria/text are tried),
  - `-strategy infinite`: infinite scroll (no button; the loop stops when the
    count stops growing),
  - `-strategy page` (+ `-page-param <name>`, default `page`): navigate
    `?page=N` until a page has no grid.
  Known hosts pick a default via `autoscrape::default_strategy` (e.g.
  `www.parfumerie.com.ar` → infinite scroll); `-strategy` overrides. Add
  `-headless` to run without a visible window (same persistent profile and
  single-instance lock as `cargo run`, so a running browser session is killed
  first). Exits when the count stops increasing, the strategy is exhausted, or
  the `MAX_STEPS` budget is spent.

## UI
- A Flutter (macOS) app lives in the sibling repo
  `../price_hunter_ui` (separate git repo, not part of this crate). It shows a
  product × provider table (rows = full product name, columns = provider
  domains, cells = latest price, blank when a provider doesn't carry the
  product) by calling `http://127.0.0.1:8091/matrix` (URL editable in-app).
- Run it: start PocketBase, then `cargo run -- -matrix-server`, then
  `cd ../price_hunter_ui && flutter run -d macos`. The macOS app needs the
  `com.apple.security.network.client` entitlement (already set in
  `macos/Runner/{DebugProfile,Release}.entitlements`) to make outbound HTTP.

## Tests
- `cargo test` runs the fixture-based detection tests (no network/browser needed)
  plus the lib unit tests. Always run `cargo test` AND `cargo clippy --all-targets`
  after changing code.
- The integration test tree is grouped by concern under `tests/`:
  - `tests/application/` — offline use-case tests that drive `application`
    (`matching`, `brands`, `matrix`) through an in-memory [`PriceStore`] fake
    (`tests/application/fakes.rs`) instead of PocketBase; aggregated by
    `tests/application.rs`, run with `cargo test --test application`.
  - `tests/sites/<site>.rs` — offline fixture tests (aggregated by `tests/sites.rs`);
    run all with `cargo test` or one site with `cargo test --test sites <site>`.
  - `tests/live/` — `#[ignore]`d tests that need network, a browser session or
    PocketBase (aggregated by `tests/live.rs`; run with
    `cargo test --test live -- --ignored`).
  - `tests/diagnostics/` — env-gated debugging tools (aggregated by
    `tests/diagnostics.rs`; no-ops without their env var).
  - `tests/common/mod.rs` — shared assertion helpers; `tests/fixtures/<site>.html`
    — saved page dumps.
- Fixture tests share `tests/common/mod.rs`
  (`assert_fixture(path, &products(), container_class)`, which checks every
  expected `Product` (name + price) exists and the container has the class).
  Expected products use `price_text: String::new()` — only name and price are
  compared.
- Live integration tests are `#[ignore]`d and require network plus a real session:
  - `cargo test --test live compreahora -- --ignored` opens Chrome using the
    persistent `profiles/chrome` session, visits the perfumeria category, and
    asserts names + prices are extracted. Set `PRICE_HUNTER_DUMP_HTML=1` to save
    the rendered page to `captures/diagnostic/` for debugging.
  - `cargo test --test live beauty24 -- --ignored` and
    `cargo test --test live fabilu -- --ignored` fetch the site over plain HTTP.
  - `cargo test --test live parfumerie -- --ignored` opens Chrome, visits the
    `/fragancias` category, scrolls to trigger infinite scroll, and asserts at
    least 10 products with names + prices.
  - `cargo test --test live store -- --ignored` requires a running PocketBase
    (see `pocketbase/scripts/setup_pocketbase.sh`) and round-trips a capture +
    products through the Record API only — verifies the store works without any
    SQL or DB-file access.
- Diagnostic tools (keep these; they are no-ops without their env var):
  - `PRICE_HUNTER_PROBE_FIXTURE=<path> cargo test --test diagnostics probe -- --nocapture`
    dumps the full `Detection` (container + products) for any HTML file.
  - `PRICE_HUNTER_MEASURE_FIXTURE=<path> cargo test --test diagnostics measure -- --nocapture`
    ranks every candidate container (p/d/density + selected flag) — use when the
    wrong container is picked.
  - `PRICE_HUNTER_LIVE_URL=<url> PRICE_HUNTER_DUMP_HTML=1 cargo test --test diagnostics live_probe -- --ignored -- --nocapture`
    opens Chrome, navigates, scrolls, optionally dumps the rendered HTML, and
    prints the detected container + products.
- Close any `cargo run` browser first — Chrome locks `profiles/chrome` while
  running. A new `cargo run` now kills the previous instance automatically via
  the PID file (`src/instance.rs`), so manual cleanup is normally unnecessary.

## Adding a new site / fixture
There is a skill and a command for this; use them instead of improvising:
- Skill: `.opencode/skills/new-fixture-test/SKILL.md` (auto-loaded by the model
  when the task matches; follow its workflow).
- Command: `/new-fixture-test tests/fixtures/<site>.html` runs the whole flow:
  map the fixture, count cards, extract expected products, probe the pipeline,
  fix `detect.rs` only at the gap (with a fallback + unit test), write
  `tests/sites/<site>.rs` (and register it in `tests/sites/mod.rs`), then verify
  `cargo test` + `cargo clippy --all-targets`.

Workflow summary (details in the skill):
1. Save the site's rendered grid as `tests/fixtures/<site>.html`.
2. Identify container, card, name, and current-price markup; count cards with
   `rg -c`.
3. Extract expected products (one-off python script; templates in the skill for
   PrestaShop and Magento/Hyva markup).
4. Probe with `tests/diagnostics/probe.rs`; if detection differs from the
   fixture, use `tests/diagnostics/measure.rs` / `live_probe.rs` to diagnose and
   fix `detect.rs` with a backward-compatible fallback.
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
  is probably holding `profiles/chrome`. `cargo run` now self-heals via the PID
  file (`src/instance.rs`); for `#[ignore]`d tests that launch their own
  browser, kill it manually (`pkill -f profiles/chrome`) and rerun.
- `.opencode/node_modules` is gitignored; don't touch or commit it.
- Keep changes backward-compatible: when extending `detect.rs`, preserve the
  existing fallbacks (`current_price_of` → `prices.last()`, `card_of` descent,
  etc.) so all existing fixture tests keep passing.
