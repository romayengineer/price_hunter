# AGENTS.md

## Project
Price Hunter — a new Rust project. No code exists yet (repo has no commits).

## Architecture intent
- Built with `thirtyfour` (Selenium WebDriver bindings for Rust).
- Opens a real browser that remains interactive and user-controlled.
- In the background, a non-blocking task inspects the current page's DOM for
  structured price patterns (grids, columns, rows) and extracts product names
  and prices into a JSON capture.
- Target users: people scraping prices from pages they already know while
  continuing to use the browser normally.

## Gotchas
- `captures/` is gitignored — write capture output there; do not expect it committed.
