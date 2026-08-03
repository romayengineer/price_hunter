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

## Run
- `cargo run` opens Chrome and keeps it open until the window is closed or Ctrl+C.
- Optional URL arg: `cargo run -- https://example.com`. Failed navigation warns
  but keeps the session alive.
- `--capture` writes the auto-detected products to `captures/capture-<timestamp>.json`
  as soon as a grid is found (re-checks on later polls if the page is still loading).

## Gotchas
- `captures/` is gitignored — write capture output there; do not expect it committed.
- Do not `driver.quit()`/drop early while the user is driving the browser; keep
  the `WebDriver` alive until the browser window closes (probe with
  `driver.current_url()`).
