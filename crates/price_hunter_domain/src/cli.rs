//! Pure CLI argument parsing: `argv` into [`Command`]. No I/O, no output —
//! the binary owns dispatch and printing.

use std::path::PathBuf;

use crate::scrape::{AutoScrapeOptions, StrategyKind};

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
    /// `-delete-products [<csv>]` — delete all canonical products or, with a
    /// CSV path, only those whose `(brand, product_name, size)` is absent from
    /// the CSV (auto-unlinks provider_products, cascade-deletes matches).
    DeleteProducts(Option<PathBuf>),
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
    /// `-version` — print binary version and exit.
    Version,
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
    if rest.iter().any(|a| a == "-version") {
        return Command::Version;
    }
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
    if rest.iter().any(|a| a == "-delete-products") {
        return Command::DeleteProducts(arg_after_optional(rest, "-delete-products"));
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
fn parse_auto_scrape(rest: &[String], url: String) -> AutoScrapeOptions {
    let strategy = arg_after_string(rest, "-strategy").map(parse_strategy_kind);
    let button = arg_after_string(rest, "-button");
    let page_param = arg_after_string(rest, "-page-param").unwrap_or_default();
    let window_threshold = parse_window_threshold(rest);
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

fn parse_strategy_kind(s: String) -> StrategyKind {
    match s.to_ascii_lowercase().as_str() {
        "scroll-click" | "scroll_click" | "scrollclick" => StrategyKind::ScrollClick,
        "infinite" | "infinite-scroll" | "scroll" => StrategyKind::InfiniteScroll,
        "page" | "pagination" => StrategyKind::Page,
        _ => StrategyKind::ScrollClick,
    }
}

fn parse_window_threshold(rest: &[String]) -> Option<usize> {
    arg_after_string(rest, "-window-threshold")
        .or_else(|| arg_after_string(rest, "-window"))
        .and_then(|s| s.parse::<usize>().ok())
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

/// The value following `flag` in `rest` as a path, if present and not another flag.
fn arg_after_optional(rest: &[String], flag: &str) -> Option<PathBuf> {
    let i = rest.iter().position(|a| a == flag)?;
    let next = rest.get(i + 1)?;
    if next.starts_with('-') {
        None
    } else {
        Some(PathBuf::from(next))
    }
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
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
        assert_eq!(parse(&args(&["-version"])), Command::Version);
    }

    #[test]
    fn version_flag_wins_over_other_flags() {
        assert_eq!(
            parse(&args(&["-version", "-match-products"])),
            Command::Version
        );
        assert_eq!(
            parse(&args(&["-matrix-server", "-version"])),
            Command::Version
        );
        assert_eq!(
            parse(&args(&["-import-brands", "b.csv", "-version"])),
            Command::Version
        );
    }

    #[test]
    fn delete_products_flag_parses_optional_path() {
        assert_eq!(
            parse(&args(&["-delete-products"])),
            Command::DeleteProducts(None)
        );
        assert_eq!(
            parse(&args(&["-delete-products", "products.csv"])),
            Command::DeleteProducts(Some(PathBuf::from("products.csv")))
        );
        // flag-like next arg is not consumed
        assert_eq!(
            parse(&args(&["-delete-products", "-yes"])),
            Command::DeleteProducts(None)
        );
        assert_eq!(
            parse(&args(&["-delete-products", "products.csv", "-yes"])),
            Command::DeleteProducts(Some(PathBuf::from("products.csv")))
        );
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
        // -delete-products is lower priority than imports
        assert_eq!(
            parse(&args(&["-delete-products", "stale.csv", "-import-products", "p.csv"])),
            Command::ImportProducts(PathBuf::from("p.csv"))
        );
    }
}
