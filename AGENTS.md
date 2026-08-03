# AGENTS.md

## Project
Price Hunter — a Rust binary using `thirtyfour` (Selenium WebDriver for Rust).
Current state: minimal v1 — opens a real browser and lets the user control it.

## Architecture intent
- Opens a real browser that remains interactive and user-controlled.
- In the background, a non-blocking task inspects the current page's DOM for
  structured price patterns (grids, columns, rows) and extracts product names
  and prices into a JSON capture (not implemented yet).
- Target users: people scraping prices from pages they already know while
  continuing to use the browser normally.

## Toolchain
- `cargo`/`rustc` are NOT on PATH (`~/.cargo/bin/cargo` and `rustup` are broken
  symlinks). Use the toolchain binary directly:
  `export PATH="/Users/macbookpro/.rustup/toolchains/stable-x86_64-apple-darwin/bin:$PATH"`
  then `cargo ...`. (`rustup run stable cargo` fails with a rustc-not-found error.)
- No chromedriver needs installing: `WebDriver::managed` auto-downloads the
  matching chromedriver on first run and shuts it down on exit.

## Run
- `cargo run` opens Chrome and keeps it open until the window is closed or Ctrl+C.
- Optional URL arg: `cargo run -- https://example.com`. Failed navigation warns
  but keeps the session alive.

## Gotchas
- `captures/` is gitignored — write capture output there; do not expect it committed.
- Do not `driver.quit()`/drop early while the user is driving the browser; keep
  the `WebDriver` alive until the browser window closes (probe with
  `driver.current_url()`).
