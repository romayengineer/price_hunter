//! Binary-side CLI: parses `argv` into a [`Command`], connects the store, and
//! owns all user-facing output. The application layer stays silent — progress
//! flows through [`Reporter`] and results are printed here.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use thirtyfour::prelude::*;

use price_hunter::application::reporter::Reporter;
use price_hunter::application::{brands, imports, matching, matrix};
use price_hunter::browser;
use price_hunter::capture;
use price_hunter::config;
use price_hunter::detect::{self, Detection, Product};
use price_hunter::domain::model::ProductInsert;
use price_hunter::domain::ports::PriceStore;
use price_hunter::export;
use price_hunter::instance::InstanceGuard;
use price_hunter::store::Store;

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
    Command::Browse(rest.iter().find(|a| !a.starts_with('-')).cloned())
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
        Command::Browse(url) => browse(url).await,
    }
}

/// Returns `true` when `yes` was set or the user answered `y`/`yes` to
/// `prompt`. With `yes` set no input is read, so a non-interactive run never
/// hangs waiting on stdin.
fn confirm(prompt: &str, yes: bool) -> anyhow::Result<bool> {
    if yes {
        return Ok(true);
    }
    use std::io::Write;
    print!("{prompt} ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    let answer = answer.trim();
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

/// Returns `Ok(true)` for a single `y` key press (no Enter needed), `Ok(false)`
/// for anything else. With `yes` set no key is read. Falls back to a full line
/// when stdin is not a terminal.
fn confirm_key(prompt: &str, yes: bool) -> anyhow::Result<bool> {
    if yes {
        return Ok(true);
    }
    use std::io::Write;
    print!("{prompt} ");
    std::io::stdout().flush()?;
    let key = read_single_key()?;
    println!();
    Ok(key == b'y' || key == b'Y')
}

/// Reads one byte from stdin with the terminal in raw mode (no echo, no Enter
/// required) and restores it afterwards. When stdin is not a TTY (piped input,
/// tests), falls back to a line read and takes its first byte.
fn read_single_key() -> anyhow::Result<u8> {
    use std::io::Read;
    use std::os::fd::AsRawFd;

    let mut stdin = std::io::stdin();
    let fd = stdin.as_raw_fd();
    let raw = termios::Termios::from_fd(fd)
        .and_then(|mut t| {
            t.c_lflag &= !(termios::ICANON | termios::ECHO);
            t.c_cc[termios::VMIN] = 1;
            t.c_cc[termios::VTIME] = 0;
            termios::tcsetattr(fd, termios::TCSANOW, &t)?;
            Ok(t)
        });
    let mut buf = [0u8; 1];
    let result: std::io::Result<u8> = match raw {
        Ok(original) => {
            let n = stdin.read(&mut buf);
            let _ = termios::tcsetattr(fd, termios::TCSANOW, &original);
            n.map(|count| if count == 0 { b'\n' } else { buf[0] })
        }
        Err(_) => {
            let mut line = String::new();
            match stdin.read_line(&mut line) {
                Ok(_) => Ok(line.as_bytes().first().copied().unwrap_or(b'\n')),
                Err(e) => Err(e),
            }
        }
    };
    result.map_err(Into::into)
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
            println!("Deleting page {}/{} ({} rows)", page_index + 1, pages, page.len());
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
            i + 1, total, brand, proposal.product_name, size
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

/// Drives the browser poll loop until the window is closed.
async fn run_session(driver: &WebDriver, url: Option<String>, store: Store) -> anyhow::Result<()> {
    navigate_to_arg(driver, url).await;

    println!("Browser is open and under your control. Close the window (or Ctrl+C) to exit.");

    let mut state = LoopState {
        last_source: None,
        detection: None,
        last_capture_products: None,
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
    last_capture_products: Option<Vec<Product>>,
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

async fn capture_if_needed(driver: &WebDriver, state: &mut LoopState) {
    let Some(detection) = &state.detection else {
        return;
    };
    if state.last_capture_products.as_ref() == Some(&detection.products) {
        return;
    }
    let url = driver
        .current_url()
        .await
        .map(|u| u.to_string())
        .unwrap_or_default();
    let path = match capture::write_capture("captures", &url, detection) {
        Ok(path) => path,
        Err(e) => {
            log::error!("Could not write capture for {url}: {e}");
            return;
        }
    };
    println!(
        "Captured {} products to {}",
        detection.products.len(),
        path.display()
    );
    let capture_path = path.display().to_string();
    persist_to_store(&state.store, &url, &capture_path, detection);
    state.last_capture_products = Some(detection.products.clone());
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
