//! Binary-side CLI: parses `argv` into a [`Command`], connects the store, and
//! owns all user-facing output. The application layer stays silent — progress
//! flows through [`Reporter`] and results are printed here.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use thirtyfour::prelude::*;

use price_hunter::application::reporter::Reporter;
use price_hunter::application::{brands, imports, matching, matrix};
use price_hunter::autoscrape::{self, AutoScrapeOptions, StrategyKind};
use price_hunter::browser;
use price_hunter::capture;
use price_hunter::config;
use price_hunter::detect::{self, Detection, Product};
use price_hunter::domain::model::ProductInsert;
use price_hunter::domain::ports::{BrandCatalog, ProductCatalog, ProviderCatalog};
use price_hunter::export;
use price_hunter::instance::InstanceGuard;
use price_hunter::store::Store;
use price_hunter::terminal::{confirm, confirm_key};

/// Every entry point of the binary.
#[derive(Debug, PartialEq)]
pub enum Command {
    /// `-import-products <csv>`
    ImportProducts(PathBuf),
    /// `-import-brands <csv>`
    ImportBrands(PathBuf),
    /// `-export-matrix <csv>`
    ExportMatrix(PathBuf),
    /// `-export-products <csv>`
    ExportProducts(PathBuf),
    /// `-export-brands <csv>`
    ExportBrands(PathBuf),
    /// `-match-products`
    MatchProducts,
    /// `-link-matches`
    LinkMatches,
    /// `-match-brands`
    MatchBrands,
    /// `-report-missing-brands`
    ReportMissingBrands,
    /// `-delete-unbranded`
    DeleteUnbranded,
    /// `-import-unmatched`
    ImportUnmatched,
    /// `-matrix-server`
    MatrixServer,
    /// `-auto-scrape <url> [-strategy <name>] [-button <css>] [-page-param <name>] [-window-threshold <n>] [-headless]`
    AutoScrape(AutoScrapeOptions),
    /// Default: open a browser, optionally at a URL, and poll for captures.
    Browse(Option<String>),
}

/// Parses the command line. Flag precedence matches the historical fixed
/// order; a bare URL (first non-`-` argument) selects [`Command::Browse`].
#[allow(clippy::cognitive_complexity)]
pub fn parse(args: &[String]) -> Command {
    let rest = &args[1..];
    if let Some(path) = arg_after(rest, "-import-products") {
        return Command::ImportProducts(path);
    }
    if let Some(path) = arg_after(rest, "-import-brands") {
        return Command::ImportBrands(path);
    }
    if let Some(path) = arg_after(rest, "-export-matrix") {
        return Command::ExportMatrix(path);
    }
    if let Some(path) = arg_after(rest, "-export-products") {
        return Command::ExportProducts(path);
    }
    if let Some(path) = arg_after(rest, "-export-brands") {
        return Command::ExportBrands(path);
    }
    if rest.iter().any(|a| a == "-match-products") {
        return Command::MatchProducts;
    }
    if rest.iter().any(|a| a == "-link-matches") {
        return Command::LinkMatches;
    }
    if rest.iter().any(|a| a == "-match-brands") {
        return Command::MatchBrands;
    }
    if rest.iter().any(|a| a == "-report-missing-brands") {
        return Command::ReportMissingBrands;
    }
    if rest.iter().any(|a| a == "-delete-unbranded") {
        return Command::DeleteUnbranded;
    }
    if rest.iter().any(|a| a == "-import-unmatched") {
        return Command::ImportUnmatched;
    }
    if rest.iter().any(|a| a == "-matrix-server") {
        return Command::MatrixServer;
    }
    if let Some(url) = arg_after_string(rest, "-auto-scrape") {
        return Command::AutoScrape(parse_auto_scrape(rest, url));
    }
    Command::Browse(rest.iter().find(|a| !a.starts_with('-')).cloned())
}

/// Collects the `-auto-scrape` modifier flags (`-strategy`, `-button`,
/// `-page-param`, `-window-threshold`, `-headless`) into [`AutoScrapeOptions`] for `url`.
#[allow(clippy::cognitive_complexity)]
fn parse_auto_scrape(rest: &[String], url: String) -> AutoScrapeOptions {
    let strategy =
        arg_after_string(rest, "-strategy").map(|s| match s.to_ascii_lowercase().as_str() {
            "scroll-click" | "scroll_click" | "scrollclick" => StrategyKind::ScrollClick,
            "infinite" | "infinite-scroll" | "scroll" => StrategyKind::InfiniteScroll,
            "page" | "pagination" => StrategyKind::Page,
            _ => StrategyKind::ScrollClick,
        });
    let button = arg_after_string(rest, "-button");
    let page_param = arg_after_string(rest, "-page-param").unwrap_or_default();
    let window_threshold = arg_after_string(rest, "-window-threshold")
        .or_else(|| arg_after_string(rest, "-window"))
        .and_then(|s| s.parse::<usize>().ok());
    let headless = rest.iter().any(|a| a == "-headless");
    AutoScrapeOptions {
        url,
        strategy,
        button,
        page_param,
        headless,
        window_threshold,
    }
}

/// Whether the command line carries a global auto-accept flag (`-yes`/`-y`).
/// It never selects a command by itself — it only skips interactive prompts in
/// commands that ask for confirmation.
pub fn wants_yes(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| a == "-yes" || a == "-y")
}

/// The value following `flag` in `rest`, if present.
fn arg_after(rest: &[String], flag: &str) -> Option<PathBuf> {
    let i = rest.iter().position(|a| a == flag)?;
    rest.get(i + 1).cloned().map(PathBuf::from)
}

/// The value following `flag` in `rest` as a string, if present.
fn arg_after_string(rest: &[String], flag: &str) -> Option<String> {
    let i = rest.iter().position(|a| a == flag)?;
    rest.get(i + 1).cloned()
}

/// Dispatches `command` and reports its result on stdout. `yes` auto-accepts
/// any confirmation prompt the command would otherwise ask interactively.
pub async fn run(command: Command, yes: bool) -> anyhow::Result<()> {
    match command {
        Command::ImportProducts(path) => import_products(&path),
        Command::ImportBrands(path) => import_brands(&path),
        Command::ExportMatrix(path) => export_matrix(&path),
        Command::ExportProducts(path) => export_products(&path),
        Command::ExportBrands(path) => export_brands(&path),
        Command::MatchProducts => match_products(),
        Command::LinkMatches => link_matches(),
        Command::MatchBrands => match_brands(),
        Command::ReportMissingBrands => report_missing_brands(),
        Command::DeleteUnbranded => delete_unbranded(yes),
        Command::ImportUnmatched => import_unmatched(yes),
        Command::MatrixServer => matrix_server().await,
        Command::AutoScrape(options) => auto_scrape(&options).await,
        Command::Browse(url) => browse(url).await,
    }
}

/// Connects to PocketBase, writing the config template first if needed.
fn connect() -> anyhow::Result<Store> {
    config::Config::ensure_template();
    Store::connect().context("cannot connect to PocketBase")
}

/// Imports `brand,name,size` rows from a CSV into the `products` table and
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
    let csv = export::matrix_to_csv(&matrix)?;
    std::fs::write(path, csv).with_context(|| format!("could not write CSV to {path:?}"))?;
    println!(
        "Exported {} products × {} providers to {}",
        matrix.rows.len(),
        matrix.providers.len(),
        path.display()
    );
    Ok(())
}

/// Writes the canonical products (`brand,product_name,size` columns) to a CSV
/// file and exits without opening a browser.
fn export_products(path: &PathBuf) -> anyhow::Result<()> {
    let store = connect()?;
    let products = store.list_all_products()?;
    let csv = export::products_to_csv(&products)?;
    std::fs::write(path, csv).with_context(|| format!("could not write CSV to {path:?}"))?;
    println!("Exported {} products to {}", products.len(), path.display());
    Ok(())
}

/// Writes the canonical brands (single `brand` column) to a CSV file and
/// exits without opening a browser.
fn export_brands(path: &PathBuf) -> anyhow::Result<()> {
    let store = connect()?;
    let brands = store.list_brands()?;
    let csv = export::brands_to_csv(&brands)?;
    std::fs::write(path, csv).with_context(|| format!("could not write CSV to {path:?}"))?;
    println!("Exported {} brands to {}", brands.len(), path.display());
    Ok(())
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
        let size = if proposal.size.is_empty() {
            "-"
        } else {
            &proposal.size
        };
        println!(
            "{}/{}  {} | {} | {}",
            i + 1,
            total,
            brand,
            proposal.product_name,
            size
        );
        println!("      from: {}", proposal.source_name);
        if !confirm_key("Insert as canonical product? (y/N)", yes)? {
            skipped += 1;
            println!("      skipped");
            continue;
        }
        match store.create_product(
            &proposal.brand,
            &proposal.product_name,
            &proposal.name,
            &proposal.size,
        )? {
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
        strategy_kind_name(autoscrape::effective_strategy(url, options))
    );

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let detection = autoscrape::scrape_until_no_growth(
        driver,
        strategy.as_mut(),
        autoscrape::SETTLE,
        autoscrape::MAX_STEPS,
        |detection| {
            let new_products = price_hunter::detect::product_delta(&detection.products, &mut seen);
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
        let new_products = price_hunter::detect::product_delta(&detection.products, &mut seen);
        if !new_products.is_empty() {
            persist_new_products(store, url, &new_products);
        }
    }
    println!("Scraped {} products from {url}", count_of(&detection));
    Ok(count_of(&detection))
}

/// The number of products in an optional detection (0 when none).
fn count_of(detection: &Option<Detection>) -> usize {
    detection.as_ref().map_or(0, |d| d.products.len())
}

/// Writes a JSON capture for only the delta products and persists them to
/// PocketBase (delta only, `product_count = delta.len()`). A failed
/// write/save is logged, never fatal — the scrape continues. In-memory only,
/// so restarts re-emit all products as new.
#[allow(clippy::cognitive_complexity)]
fn persist_new_products(store: &Store, url: &str, new_products: &[Product]) {
    if new_products.is_empty() {
        return;
    }
    log::info!("new products {}", new_products.len());
    let detection = Detection {
        container: price_hunter::detect::Container {
            classes: Vec::new(),
            id: None,
            child_count: new_products.len(),
        },
        products: new_products.to_vec(),
    };
    let path = match capture::write_capture("captures", url, &detection) {
        Ok(path) => path,
        Err(e) => {
            log::error!("Could not write capture for {url}: {e}");
            return;
        }
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
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

/// A short display name for a strategy kind, for user-facing output.
fn strategy_kind_name(kind: StrategyKind) -> &'static str {
    match kind {
        StrategyKind::ScrollClick => "scroll-and-click",
        StrategyKind::InfiniteScroll => "infinite scroll",
        StrategyKind::Page => "page parameter",
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

fn update_state(state: &mut LoopState, source: Option<String>) {
    let Some(source) = source else {
        return;
    };
    if state.last_source.as_deref() == Some(source.as_str()) {
        return;
    }
    state.last_source = Some(source.clone());
    if let Some(detection) = detect::detect_grid(&source) {
        state.detection = Some(detection);
    }
}

#[allow(clippy::cognitive_complexity)]
async fn capture_if_needed(driver: &WebDriver, state: &mut LoopState) {
    let Some(detection) = &state.detection else {
        return;
    };
    // Delta only: compute new products by full identity (in-memory seen).
    let delta = price_hunter::detect::product_delta(&detection.products, &mut state.seen);
    if delta.is_empty() {
        return;
    }
    log::info!("new products {}", delta.len());
    let url = driver
        .current_url()
        .await
        .map(|u| u.to_string())
        .unwrap_or_default();
    let mut container = detection.container.clone();
    container.child_count = delta.len();
    let delta_detection = Detection {
        container,
        products: delta.clone(),
    };
    let path = match capture::write_capture("captures", &url, &delta_detection) {
        Ok(path) => path,
        Err(e) => {
            log::error!("Could not write capture for {url}: {e}");
            // Roll back seen insertions for this failed batch so retry can emit again.
            for p in &delta {
                state.seen.remove(&p.delta_key());
            }
            return;
        }
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
        if total == 0 {
            println!();
            return;
        }
        let pct = done as f64 * 100.0 / total as f64;
        if (pct - self.last_pct).abs() < 0.005 {
            return;
        }
        self.last_pct = pct;
        use std::io::Write;
        print!("\rProgress: {pct:.2}%");
        let _ = std::io::stdout().flush();
        if done == total {
            println!();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use price_hunter::terminal::{confirm, confirm_key};

    fn args(rest: &[&str]) -> Vec<String> {
        std::iter::once("pricehunter".to_string())
            .chain(rest.iter().map(|a| a.to_string()))
            .collect()
    }

    #[test]
    fn no_args_browses_without_a_url() {
        assert_eq!(parse(&args(&[])), Command::Browse(None));
    }

    #[test]
    fn bare_url_selects_browse() {
        assert_eq!(
            parse(&args(&["https://www.parfumerie.com.ar/fragancias"])),
            Command::Browse(Some("https://www.parfumerie.com.ar/fragancias".to_string()))
        );
    }

    #[test]
    #[allow(clippy::cognitive_complexity)]
    fn import_flags_take_the_following_value() {
        assert_eq!(
            parse(&args(&["-import-products", "products.csv"])),
            Command::ImportProducts(PathBuf::from("products.csv"))
        );
        assert_eq!(
            parse(&args(&["-import-brands", "brands.csv"])),
            Command::ImportBrands(PathBuf::from("brands.csv"))
        );
        assert_eq!(
            parse(&args(&["-export-matrix", "matrix.csv"])),
            Command::ExportMatrix(PathBuf::from("matrix.csv"))
        );
        assert_eq!(
            parse(&args(&["-export-products", "products.csv"])),
            Command::ExportProducts(PathBuf::from("products.csv"))
        );
        assert_eq!(
            parse(&args(&["-export-brands", "brands.csv"])),
            Command::ExportBrands(PathBuf::from("brands.csv"))
        );
    }

    #[test]
    #[allow(clippy::cognitive_complexity)]
    fn bare_flags_dispatch_to_their_command() {
        assert_eq!(parse(&args(&["-match-products"])), Command::MatchProducts);
        assert_eq!(parse(&args(&["-link-matches"])), Command::LinkMatches);
        assert_eq!(parse(&args(&["-match-brands"])), Command::MatchBrands);
        assert_eq!(
            parse(&args(&["-report-missing-brands"])),
            Command::ReportMissingBrands
        );
        assert_eq!(
            parse(&args(&["-delete-unbranded"])),
            Command::DeleteUnbranded
        );
        assert_eq!(
            parse(&args(&["-import-unmatched"])),
            Command::ImportUnmatched
        );
        assert_eq!(parse(&args(&["-matrix-server"])), Command::MatrixServer);
    }

    #[test]
    fn auto_scrape_flag_collects_url_and_modifiers() {
        assert_eq!(
            parse(&args(&["-auto-scrape", "https://example.com/list"])),
            Command::AutoScrape(AutoScrapeOptions {
                url: "https://example.com/list".to_string(),
                strategy: None,
                button: None,
                page_param: String::new(),
                headless: false,
                window_threshold: None,
            })
        );
        assert_eq!(
            parse(&args(&[
                "-auto-scrape",
                "https://example.com/list",
                "-strategy",
                "page",
                "-page-param",
                "pg",
                "-button",
                ".load-more",
                "-headless"
            ])),
            Command::AutoScrape(AutoScrapeOptions {
                url: "https://example.com/list".to_string(),
                strategy: Some(StrategyKind::Page),
                button: Some(".load-more".to_string()),
                page_param: "pg".to_string(),
                headless: true,
                window_threshold: None,
            })
        );
        assert_eq!(
            parse(&args(&["-auto-scrape", "u", "-strategy", "infinite"])),
            Command::AutoScrape(AutoScrapeOptions {
                url: "u".to_string(),
                strategy: Some(StrategyKind::InfiniteScroll),
                button: None,
                page_param: String::new(),
                headless: false,
                window_threshold: None,
            })
        );
    }

    #[test]
    fn auto_scrape_window_threshold_parses_and_defaults() {
        assert_eq!(
            parse(&args(&[
                "-auto-scrape",
                "https://example.com/list?page=1",
                "-window-threshold",
                "200"
            ])),
            Command::AutoScrape(AutoScrapeOptions {
                url: "https://example.com/list?page=1".to_string(),
                strategy: None,
                button: None,
                page_param: String::new(),
                headless: false,
                window_threshold: Some(200),
            })
        );
        // alias -window
        assert_eq!(
            parse(&args(&[
                "-auto-scrape",
                "https://example.com/list?page=1",
                "-window",
                "50"
            ])),
            Command::AutoScrape(AutoScrapeOptions {
                url: "https://example.com/list?page=1".to_string(),
                strategy: None,
                button: None,
                page_param: String::new(),
                headless: false,
                window_threshold: Some(50),
            })
        );
        // 0 disables
        assert_eq!(
            parse(&args(&[
                "-auto-scrape",
                "https://example.com/list?page=1",
                "-window-threshold",
                "0"
            ])),
            Command::AutoScrape(AutoScrapeOptions {
                url: "https://example.com/list?page=1".to_string(),
                strategy: None,
                button: None,
                page_param: String::new(),
                headless: false,
                window_threshold: Some(0),
            })
        );
        // invalid value is ignored (fallback to None => default 120)
        assert_eq!(
            parse(&args(&[
                "-auto-scrape",
                "https://example.com/list?page=1",
                "-window-threshold",
                "bad"
            ])),
            Command::AutoScrape(AutoScrapeOptions {
                url: "https://example.com/list?page=1".to_string(),
                strategy: None,
                button: None,
                page_param: String::new(),
                headless: false,
                window_threshold: None,
            })
        );
    }

    #[test]
    fn flags_win_over_a_bare_url() {
        assert_eq!(
            parse(&args(&["-matrix-server", "https://example.com"])),
            Command::MatrixServer
        );
    }

    #[test]
    fn yes_flag_does_not_select_a_command() {
        assert_eq!(
            parse(&args(&["-delete-unbranded", "-yes"])),
            Command::DeleteUnbranded
        );
        assert_eq!(
            parse(&args(&["-y", "-delete-unbranded"])),
            Command::DeleteUnbranded
        );
    }

    #[test]
    fn wants_yes_recognizes_yes_and_y() {
        assert!(wants_yes(&args(&["-delete-unbranded", "-yes"])));
        assert!(wants_yes(&args(&["-delete-unbranded", "-y"])));
        assert!(!wants_yes(&args(&["-delete-unbranded"])));
        assert!(!wants_yes(&args(&[])));
    }

    #[test]
    fn confirm_with_yes_returns_true_without_reading_stdin() {
        assert!(confirm("Delete these rows? [y/N]", true).unwrap());
    }

    #[test]
    fn confirm_key_with_yes_returns_true_without_reading_stdin() {
        assert!(confirm_key("Insert as canonical product? (y/N)", true).unwrap());
    }

    #[test]
    fn flag_precedence_follows_the_fixed_order() {
        // A later-declared flag is chosen over an earlier one only by the
        // documented precedence, not argv order.
        assert_eq!(
            parse(&args(&["-match-products", "-import-brands", "b.csv"])),
            Command::ImportBrands(PathBuf::from("b.csv"))
        );
        assert_eq!(
            parse(&args(&[
                "-import-brands",
                "b.csv",
                "-import-products",
                "p.csv"
            ])),
            Command::ImportProducts(PathBuf::from("p.csv"))
        );
    }
}
