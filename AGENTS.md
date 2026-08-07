# AGENTS.md

## Project
Price Hunter — a Rust binary using `thirtyfour` (Selenium WebDriver for Rust).
Current state: minimal v1 — opens a real browser and lets the user control it.

## Architecture intent
- Opens a real browser that remains interactive and user-controlled.
- In the background, the poll loop fetches the live page HTML (via
  `driver.source()`) and inspects it for structured price patterns (grids,
  columns, rows), extracting product names and prices into a JSON capture.
- Target users: people scraping prices from pages they already know while
  continuing to use the browser normally.

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
  detected products (prices or names) change on a later poll.

## Tests
- `cargo test` runs the fixture-based detection tests (no network/browser needed).
- Live integration tests are `#[ignore]`d and require network plus a real session:
  - `cargo test --test compreahora_live -- --ignored` opens Chrome using the
    persistent `profiles/chrome` session, visits the perfumeria category, and
    asserts names + prices are extracted. Set `PRICE_HUNTER_DUMP_HTML=1` to save
    the rendered page to `captures/diagnostic/` for debugging.
  - `cargo test --test fabilu -- --ignored` fetches fabilu over plain HTTP.
  - `cargo test --test parfumerie_live -- --ignored` opens Chrome, visits the
    `/fragancias` category, scrolls to trigger infinite scroll, and asserts at
    least 10 products with names + prices.
- Close any `cargo run` browser first — Chrome locks `profiles/chrome` while running.

## Gotchas
- `captures/` is gitignored — write capture output there; do not expect it committed.
- Do not `driver.quit()`/drop early while the user is driving the browser; keep
  the `WebDriver` alive until the browser window closes (probe with
  `driver.current_url()`).
