//! Binary-side CLI: dispatches [`Command`], connects the store, and
//! owns all user-facing output. Argument parsing lives in
//! `price_hunter_domain::cli`; the application layer stays silent — progress
//! flows through [`Reporter`] and results are printed here.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use thirtyfour::prelude::*;

pub use price_hunter_domain::cli::{Command, parse, wants_yes};
use price_hunter_domain::reporter::Reporter;
use price_hunter_domain::scrape::AutoScrapeOptions;
use price_hunter_domain::usecases::{brands, imports, matching, matrix};
use price_hunter::autoscrape;
use price_hunter::browser;
use price_hunter::capture;
use price_hunter::config;
use price_hunter::detect::{self, Detection, Product};
use price_hunter_domain::model::ProductInsert;
use price_hunter_domain::ports::{BrandCatalog, ProductCatalog, ProviderCatalog};
use price_hunter::export;
use price_hunter::instance::InstanceGuard;
use price_hunter::store::Store;
use price_hunter::terminal::{confirm, confirm_key};

/// Dispatches `command` and reports its result on stdout. `yes` auto-accepts
/// any confirmation prompt the command would otherwise ask interactively.
pub async fn run(command: Command, yes: bool) -> anyhow::Result<()> {
    match command {
        Command::ImportProducts(path) => import_products(&path),
        Command::ImportBrands(path) => import_brands(&path),
        Command::ExportMatrix(path) => export_matrix(&path),
        Command::ExportProducts(path) => export_products(&path),
        Command::ExportBrands(path) => export_brands(&path),
        Command::DeleteProducts(path) => delete_products(path, yes),
        Command::MatchProducts => match_products(),
        Command::LinkMatches => link_matches(),
        Command::MatchBrands => match_brands(),
        Command::ReportMissingBrands => report_missing_brands(),
        Command::DeleteUnbranded => delete_unbranded(yes),
        Command::ImportUnmatched => import_unmatched(yes),
        Command::MatrixServer => matrix_server().await,
        Command::Version => print_version(),
        Command::AutoScrape(options) => auto_scrape(&options).await,
        Command::Browse(url) => browse(url).await,
    }
}

/// Connects to PocketBase, writing the config template first if needed.
fn connect() -> anyhow::Result<Store> {
    config::Config::ensure_template();
    Store::connect().context("cannot connect to PocketBase")
}

/// Prints the binary version and exits without side effects (no PocketBase,
/// no browser, no config writes).
fn print_version() -> anyhow::Result<()> {
    println!("pricehunter {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

/// Imports `brand,product_name` rows from a CSV into the `products` table and
/// exits without opening a browser.
fn import_products(path: &std::path::Path) -> anyhow::Result<()> {
    let store = connect()?;
    let created = store.import_products_csv(path)?;
    println!("Done: {created} products imported");
    Ok(())
}

/// Imports the canonical brand list (single CSV column) into the `brand`
/// table and exits without opening a browser.
fn import_brands(path: &std::path::Path) -> anyhow::Result<()> {
    let store = connect()?;
    let created = store.import_brands_csv(path)?;
    println!("Done: {created} brands imported");
    Ok(())
}

/// Writes the product × provider price matrix (same table the matrix server
/// serves) to a CSV file and exits without opening a browser.
fn export_matrix(path: &PathBuf) -> anyhow::Result<()> {
    let store = connect()?;
    let matrix = matrix::matrix(&store)?;
    let csv = export::matrix_to_csv(&matrix);
    std::fs::write(path, csv).with_context(|| format!("could not write CSV to {path:?}"))?;
    println!(
        "Exported {} products × {} providers to {}",
        matrix.rows.len(),
        matrix.providers.len(),
        path.display()
    );
    Ok(())
}

/// Writes the canonical products (`brand,product_name` columns) to a CSV
/// file and exits without opening a browser.
fn export_products(path: &PathBuf) -> anyhow::Result<()> {
    let store = connect()?;
    let products = store.list_all_products()?;
    let csv = export::products_to_csv(&products);
    std::fs::write(path, csv).with_context(|| format!("could not write CSV to {path:?}"))?;
    println!("Exported {} products to {}", products.len(), path.display());
    Ok(())
}

/// Writes the canonical brands (single `brand` column) to a CSV file and
/// exits without opening a browser.
fn export_brands(path: &PathBuf) -> anyhow::Result<()> {
    let store = connect()?;
    let brands = store.list_brands()?;
    let csv = export::brands_to_csv(&brands);
    std::fs::write(path, csv).with_context(|| format!("could not write CSV to {path:?}"))?;
    println!("Exported {} brands to {}", brands.len(), path.display());
    Ok(())
}

/// Deletes canonical products. With a CSV path, only products whose
/// `(brand, product_name)` is absent from the CSV are removed;
/// without a path, every product is removed. Each stale product is
/// auto-unlinked from provider_products (kept unlinked) and its
/// provider_product_matches are cascade-deleted before the product row
/// itself is deleted. Prompts per 50-row page unless `yes` is set.
fn delete_products(path: Option<PathBuf>, yes: bool) -> anyhow::Result<()> {
    let store = connect()?;
    let stale = stale_products(&store, path.as_ref())?;
    if stale.is_empty() {
        println!("Nothing to delete");
        return Ok(());
    }
    delete_product_pages(&store, &stale, yes)
}

fn stale_products(
    store: &Store,
    path: Option<&PathBuf>,
) -> anyhow::Result<Vec<price_hunter_domain::model::ProductRow>> {
    let all = store.list_all_products()?;
    let keys = path.map(|p| csv_product_keys(p.as_path())).transpose()?;
    Ok(price_hunter_domain::usecases::prune::stale_products(
        all,
        keys.as_ref(),
    ))
}

fn delete_product_pages(
    store: &Store,
    stale: &[price_hunter_domain::model::ProductRow],
    yes: bool,
) -> anyhow::Result<()> {
    println!("{} canonical products to delete", stale.len());
    let pages = price_hunter_domain::usecases::prune::page_count(stale.len());
    let mut deleted = 0usize;
    for (page_index, page) in stale
        .chunks(price_hunter_domain::usecases::prune::DELETE_PAGE_SIZE)
        .enumerate()
    {
        if !confirm_delete_page(page, page_index, pages, yes, deleted, stale.len())? {
            return Ok(());
        }
        for row in page {
            store
                .delete_product(&row.id)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        }
        deleted += page.len();
        println!("Deleted {} rows (total {deleted})", page.len());
    }
    println!("Done: deleted {deleted} canonical products");
    Ok(())
}

fn confirm_delete_page(
    page: &[price_hunter_domain::model::ProductRow],
    page_index: usize,
    pages: usize,
    yes: bool,
    deleted: usize,
    total: usize,
) -> anyhow::Result<bool> {
    println!();
    if yes {
        println!(
            "Deleting page {}/{} ({} rows)",
            page_index + 1,
            pages,
            page.len()
        );
        return Ok(true);
    }
    println!("Next page ({} rows):", page.len());
    for (i, row) in page.iter().enumerate() {
        println!("{}. {}\t{} [{}]", i + 1, row.id, row.name, row.brand);
    }
    if !confirm(&format!("Delete these {} rows? [y/N]", page.len()), false)? {
        println!("Aborted ({} of {} deleted)", deleted, total);
        return Ok(false);
    }
    Ok(true)
}

/// Reads `(brand, product_name)` keys from a `brand,product_name`
/// CSV (header-aware via `csv::Reader`; row parsing lives in
/// `domain::usecases::imports::parse_product_key`).
fn csv_product_keys(path: &std::path::Path) -> anyhow::Result<std::collections::HashSet<(String, String)>> {
    let mut reader = csv::Reader::from_path(path)
        .with_context(|| format!("could not read CSV at {}", path.display()))?;
    let mut keys = std::collections::HashSet::new();
    for result in reader.records() {
        let record = result.with_context(|| format!("could not parse CSV at {}", path.display()))?;
        if let Some(key) = price_hunter_domain::usecases::imports::parse_product_key(
            record.get(0).unwrap_or_default(),
            record.get(1).unwrap_or_default(),
        ) {
            keys.insert(key);
        }
    }
    Ok(keys)
}

/// Runs the fuzzy matcher against the `products` and `provider_products`
/// tables and exits without opening a browser.
fn match_products() -> anyhow::Result<()> {
    let store = connect()?;
    let summary = matching::match_products(&store, &mut StdoutReporter::new())?;
    println!(
        "Stored {} new matches ({} already stored)",
        summary.computed, summary.already_stored
    );
    println!(
        "Matched {} of {} provider products",
        summary.matched, summary.provider_products
    );
    println!("Done: {} provider products matched", summary.matched);
    Ok(())
}

/// Re-links provider products from already-stored comparisons (no backfill)
/// and exits without opening a browser.
fn link_matches() -> anyhow::Result<()> {
    let store = connect()?;
    let summary = matching::link_matches(&store)?;
    println!(
        "Matched {} of {} provider products",
        summary.matched, summary.provider_products
    );
    println!("Done: {} provider products matched", summary.matched);
    Ok(())
}

/// Assigns a brand to every provider product (`provider_products.brand_id`,
/// from the linked product's brand or a fuzzy brand match) and exits without
/// opening a browser.
fn match_brands() -> anyhow::Result<()> {
    let store = connect()?;
    let summary = brands::match_brands(&store)?;
    let matched = summary.matched_from_product + summary.matched_by_fuzzy;
    println!(
        "Brand-matched {matched} of {} provider products (product: {}, fuzzy: {}; {} updated)",
        summary.provider_products,
        summary.matched_from_product,
        summary.matched_by_fuzzy,
        summary.updated
    );
    println!("Unmatched (brand_id null): {}", summary.unmatched);
    println!("Done: brand matching complete");
    Ok(())
}

/// Lists provider products linked to a canonical product whose stored name is
/// missing that product's brand (a likely extractor bug) and exits.
fn report_missing_brands() -> anyhow::Result<()> {
    let store = connect()?;
    let report = brands::missing_brands(&store)?;
    println!(
        "{} of {} matched provider products are missing the linked brand in their name",
        report.affected.len(),
        report.matched
    );
    for row in &report.affected {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            row.provider_domain, row.name, row.brand, row.product_id, row.provider_product_id
        );
    }
    println!("Done: {} rows affected", report.affected.len());
    Ok(())
}

/// Lists provider products whose name contains no known brand and deletes
/// them in pages of 50, asking for confirmation before each page (`y` deletes
/// the page and continues; anything else aborts). With `yes` set the rows are
/// deleted without any prompt. Exits without opening a browser.
fn delete_unbranded(yes: bool) -> anyhow::Result<()> {
    let store = connect()?;
    let rows = brands::unbranded_products(&store)?;
    println!(
        "{} provider products have no brand in their name",
        rows.len()
    );
    if rows.is_empty() {
        println!("Nothing to delete");
        return Ok(());
    }
    let pages = rows.len().div_ceil(50);
    let mut deleted = 0usize;
    for (page_index, page) in rows.chunks(50).enumerate() {
        println!();
        if yes {
            println!(
                "Deleting page {}/{} ({} rows)",
                page_index + 1,
                pages,
                page.len()
            );
        } else {
            println!("Next page ({} rows):", page.len());
            for (i, row) in page.iter().enumerate() {
                println!("{}. {}\t{}", i + 1, row.id, row.name);
            }
            if !confirm(&format!("Delete these {} rows? [y/N]", page.len()), false)? {
                println!("Aborted ({} of {} deleted)", deleted, rows.len());
                return Ok(());
            }
        }
        for row in page {
            store.delete_provider_product(&row.id)?;
        }
        deleted += page.len();
        println!("Deleted {} rows (total {deleted})", page.len());
    }
    println!("Done: deleted {deleted} provider products");
    Ok(())
}

/// Proposes canonical products from unmatched provider products (no
/// `product_id`) and inserts each one after an interactive single-key `(y/N)`
/// confirmation. `y` inserts, anything else skips. With `yes` set every
/// proposal is inserted without prompting. Exits without opening a browser.
#[allow(clippy::cognitive_complexity)]
fn import_unmatched(yes: bool) -> anyhow::Result<()> {
    let store = connect()?;
    let proposals = imports::propose_unmatched(&store)?;
    println!(
        "{} unmatched provider products become canonical product proposals",
        proposals.len()
    );
    if proposals.is_empty() {
        println!("Nothing to propose");
        return Ok(());
    }
    let total = proposals.len();
    let mut inserted = 0usize;
    let mut skipped = 0usize;
    for (i, proposal) in proposals.iter().enumerate() {
        println!();
        let brand = if proposal.brand.is_empty() {
            "?"
        } else {
            &proposal.brand
        };
        println!(
            "{}/{}  {} | {}",
            i + 1,
            total,
            brand,
            proposal.product_name
        );
        println!("      from: {}", proposal.source_name);
        if !confirm_key("Insert as canonical product? (y/N)", yes)? {
            skipped += 1;
            println!("      skipped");
            continue;
        }
        match store.create_product(&proposal.brand, &proposal.product_name, &proposal.name)? {
            ProductInsert::Created => {
                inserted += 1;
                println!("      inserted: {}", proposal.name);
            }
            ProductInsert::AlreadyExists => {
                skipped += 1;
                println!("      already exists");
            }
        }
    }
    println!();
    println!("Done: inserted {inserted}, skipped {skipped} of {total}");
    Ok(())
}

/// Serves the product × provider price matrix on http://127.0.0.1:8091 and
/// keeps running until interrupted.
async fn matrix_server() -> anyhow::Result<()> {
    let store = connect()?;
    price_hunter::matrix_server::serve(store).await
}

/// Opens a real, user-controlled browser and polls it for captures in the
/// background, persisting each new grid to a JSON file and to PocketBase.
async fn browse(url: Option<String>) -> anyhow::Result<()> {
    let store = connect()?;
    println!("Persisting captures to PocketBase via its API");
    let _instance = InstanceGuard::acquire().context("cannot take single-instance lock")?;
    let driver = browser::launch().await?;
    run_session(&driver, url, store).await?;
    driver.quit().await.map_err(Into::into)
}

/// Automatically scrapes a listing page to completion: drives the site-specific
/// [`AutoScraper`] strategy (scroll+click, infinite scroll, or `page=N`) until
/// the detected product count stops increasing, then persists the largest grid
/// to a JSON file and to PocketBase. Runs headless or visible per `options`.
async fn auto_scrape(options: &AutoScrapeOptions) -> anyhow::Result<()> {
    if options.url.is_empty() {
        anyhow::bail!("-auto-scrape requires a URL argument");
    }
    let store = connect()?;
    let _instance = InstanceGuard::acquire().context("cannot take single-instance lock")?;
    let driver = browser::launch_with(options.headless).await?;
    let result = auto_scrape_with_driver(&driver, options, &store).await;
    let quit = driver.quit().await;
    result?;
    quit.map_err(Into::into)
}

/// Runs the auto-scrape loop on an already-launched `driver`, persisting each
/// batch of newly detected products as the scrape progresses. Returns the
/// number of products scraped.
async fn auto_scrape_with_driver(
    driver: &WebDriver,
    options: &AutoScrapeOptions,
    store: &Store,
) -> anyhow::Result<usize> {
    let url = &options.url;
    println!("Navigating to {url}");
    driver
        .goto(url)
        .await
        .with_context(|| format!("could not navigate to {url}"))?;

    let mut strategy = autoscrape::strategy_for(url, options);
    println!(
        "Auto-scraping {url} with {} strategy",
        price_hunter_domain::scrape::strategy_kind_name(price_hunter_domain::scrape::effective_strategy(url, options))
    );

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let detection = autoscrape::scrape_until_no_growth_with_store(
        driver,
        strategy.as_mut(),
        price_hunter_domain::scrape::SETTLE,
        price_hunter_domain::scrape::MAX_STEPS,
        Some(store),
        |detection| {
            let new_products = price_hunter_domain::model::product_delta(&detection.products, &mut seen);
            if new_products.is_empty() {
                return;
            }
            persist_new_products(store, url, &new_products);
        },
    )
    .await?;

    // Persist any products seen in the final detection that weren't saved by a
    // growth callback (e.g. strategy exhaustion before a growth).
    if let Some(detection) = &detection {
        let new_products = price_hunter_domain::model::product_delta(&detection.products, &mut seen);
        if !new_products.is_empty() {
            persist_new_products(store, url, &new_products);
        }
    }
    println!("Scraped {} products from {url}", price_hunter_domain::capture::count_of(&detection));
    Ok(price_hunter_domain::capture::count_of(&detection))
}

/// Writes a JSON capture for only the delta products and persists them to
/// PocketBase (delta only, `product_count = delta.len()`). A failed
/// write/save is logged, never fatal — the scrape continues. In-memory only,
/// so restarts re-emit all products as new.
fn persist_new_products(store: &Store, url: &str, new_products: &[Product]) {
    if new_products.is_empty() {
        return;
    }
    log::info!("new products {}", new_products.len());
    let detection = price_hunter_domain::capture::build_delta_detection(new_products);
    let Some(path) = write_capture_or_log(url, &detection) else {
        return;
    };
    persist_delta_or_log(store, url, &path, new_products);
}

fn write_capture_or_log(url: &str, detection: &Detection) -> Option<std::path::PathBuf> {
    match capture::write_capture("captures", url, detection) {
        Ok(path) => Some(path),
        Err(e) => {
            log::error!("Could not write capture for {url}: {e}");
            None
        }
    }
}

fn persist_delta_or_log(
    store: &Store,
    url: &str,
    path: &std::path::Path,
    new_products: &[Product],
) {
    let now = price_hunter_domain::time::now_secs();
    match store.save_incremental(
        url,
        now,
        &path.display().to_string(),
        new_products.len(),
        new_products,
    ) {
        Ok(()) => println!(
            "Persisted {} new products to {}",
            new_products.len(),
            path.display()
        ),
        Err(e) => log::error!("Could not persist capture to the store: {e}"),
    }
}

/// Drives the browser poll loop until the window is closed.
async fn run_session(driver: &WebDriver, url: Option<String>, store: Store) -> anyhow::Result<()> {
    navigate_to_arg(driver, url).await;

    println!("Browser is open and under your control. Close the window (or Ctrl+C) to exit.");

    let mut state = LoopState {
        last_source: None,
        detection: None,
        seen: std::collections::HashSet::new(),
        store,
    };
    while !poll_closed(driver).await {
        refresh(driver, &mut state).await;
    }
    Ok(())
}

struct LoopState {
    last_source: Option<String>,
    detection: Option<Detection>,
    seen: std::collections::HashSet<String>,
    store: Store,
}

async fn navigate_to_arg(driver: &WebDriver, url: Option<String>) {
    let Some(url) = url else {
        return;
    };
    open(driver, &url).await;
}

async fn open(driver: &WebDriver, url: &str) {
    match driver.goto(url).await {
        Ok(_) => println!("Opened {url}."),
        Err(e) => eprintln!(
            "Could not navigate to {url}: {e}\nThe browser is still open — type the address there."
        ),
    }
}

async fn poll_closed(driver: &WebDriver) -> bool {
    tokio::time::sleep(Duration::from_secs(2)).await;
    driver.current_url().await.is_err()
}

async fn refresh(driver: &WebDriver, state: &mut LoopState) {
    let source = driver.source().await.ok();
    update_state(state, source);
    capture_if_needed(driver, state).await;
}

#[allow(clippy::cognitive_complexity)]
fn detect_grid_best(source: &str, store: &Store) -> Option<Detection> {
    if let Ok(brands) = store.list_brands()
        && !brands.is_empty()
        && let Some(d) = detect::detect_grid_with_brands(
            source,
            &brands.into_iter().map(|b| b.name).collect::<Vec<_>>(),
        )
    {
        return Some(d);
    }
    if let Ok(products) = store.list_products()
        && !products.is_empty()
        && let Some(d) = detect::detect_grid_with_products(source, &products)
    {
        return Some(d);
    }
    detect::detect_grid(source)
}

fn update_state(state: &mut LoopState, source: Option<String>) {
    let Some(source) = source else {
        return;
    };
    if !price_hunter_domain::capture::source_changed(state.last_source.as_deref(), &source) {
        return;
    }
    state.last_source = Some(source.clone());
    if let Some(detection) = detect_grid_best(&source, &state.store) {
        state.detection = Some(detection);
    }
}

async fn capture_if_needed(driver: &WebDriver, state: &mut LoopState) {
    let detection = match &state.detection {
        Some(d) => d.clone(),
        None => return,
    };
    let delta = price_hunter_domain::model::product_delta(&detection.products, &mut state.seen);
    if delta.is_empty() {
        return;
    }
    log::info!("new products {}", delta.len());
    let url = driver
        .current_url()
        .await
        .map(|u| u.to_string())
        .unwrap_or_default();
    let delta_detection = price_hunter_domain::capture::build_capture_delta(&detection, &delta);
    let Some(path) = write_capture_with_rollback(state, &delta, &delta_detection, &url) else {
        return;
    };
    println!(
        "Captured {} new products (of {} total) to {}",
        delta.len(),
        detection.products.len(),
        path.display()
    );
    let capture_path = path.display().to_string();
    persist_to_store(&state.store, &url, &capture_path, &delta_detection);
}

fn write_capture_with_rollback(
    state: &mut LoopState,
    delta: &[Product],
    delta_detection: &Detection,
    url: &str,
) -> Option<std::path::PathBuf> {
    match capture::write_capture("captures", url, delta_detection) {
        Ok(path) => Some(path),
        Err(e) => {
            log::error!("Could not write capture for {url}: {e}");
            price_hunter_domain::capture::rollback_seen(&mut state.seen, delta);
            None
        }
    }
}

fn persist_to_store(store: &Store, url: &str, capture_path: &str, detection: &Detection) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    match store.save(url, now, capture_path, detection) {
        Ok(()) => println!("Persisted capture to the store"),
        Err(e) => log::error!("Could not persist capture to the store: {e}"),
    }
}

/// Redraws a `Progress: X.XX%` line in place (carriage return) without
/// spamming the terminal — only rewrites when the rounded percentage changes.
struct StdoutReporter {
    last_pct: f64,
}

impl StdoutReporter {
    fn new() -> Self {
        Self { last_pct: -1.0 }
    }
}

impl Reporter for StdoutReporter {
    fn progress(&mut self, done: usize, total: usize) {
        let Some(pct) = price_hunter_domain::reporter::progress_pct(done, total) else {
            println!();
            return;
        };
        if !price_hunter_domain::reporter::should_emit_progress(self.last_pct, pct) {
            return;
        }
        self.last_pct = pct;
        use std::io::Write;
        print!("\rProgress: {pct:.2}%");
        let _ = std::io::stdout().flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn version_runs_without_side_effects() {
        assert!(run(Command::Version, false).await.is_ok());
    }

    #[test]
    fn confirm_with_yes_returns_true_without_reading_stdin() {
        use price_hunter::terminal::{confirm, confirm_key};
        assert!(confirm("Delete these rows? [y/N]", true).unwrap());
        assert!(confirm_key("Insert as canonical product? (y/N)", true).unwrap());
    }
}
