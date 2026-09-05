//! Pure auto-scrape kernel: strategy selection, termination rules and
//! load-more heuristics. No browser, no I/O — infrastructure drives these
//! values with WebDriver.

use std::time::Duration;

/// Default time to wait after a load-more action for the product count to grow
/// before considering the listing exhausted.
pub const SETTLE: Duration = Duration::from_secs(8);

/// How often to re-check the product count while waiting for a load to land.
pub const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Default number of consecutive rounds without product-count growth before the
/// listing is declared exhausted.
pub const NO_GROWTH_LIMIT: usize = 3;

/// Default maximum number of load-more steps, as a safety valve against
/// runaway infinite scroll.
pub const MAX_STEPS: usize = 200;

/// Default product count at which a `?page=N` listing reloads the same page
/// to drop earlier products from the DOM and keep memory bounded. Applies
/// only when the URL query contains the page parameter. Override with
/// `-window-threshold <n>` (0 disables windowing).
pub const DEFAULT_WINDOW_THRESHOLD: usize = 80;

/// The kind of site-specific pagination strategy to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyKind {
    /// Scroll down and click a "load more" button when it appears.
    ScrollClick,
    /// Scroll to the bottom repeatedly; new products load without a button.
    InfiniteScroll,
    /// Navigate `?page=N` for increasing N until a page has no grid.
    Page,
}

/// Options that refine how the auto-scrape strategy is built. Applied on top of
/// the host-specific default chosen by [`default_strategy`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutoScrapeOptions {
    /// The URL of the listing page to scrape.
    pub url: String,
    /// Force a strategy kind regardless of the site's registered default.
    pub strategy: Option<StrategyKind>,
    /// CSS selector for the load-more button (scroll-click strategy).
    pub button: Option<String>,
    /// Query parameter name for page-based pagination; empty means `page`.
    pub page_param: String,
    /// Whether to run the browser headless (no visible window).
    pub headless: bool,
    /// When the detected product count reaches this threshold on a `?page=N`
    /// URL, the same page is reloaded to drop earlier products from the DOM.
    /// `None` means [`DEFAULT_WINDOW_THRESHOLD`]; `Some(0)` disables windowing.
    pub window_threshold: Option<usize>,
}

impl AutoScrapeOptions {
    /// The effective page parameter name (defaults to `page`).
    pub fn page_param_name(&self) -> &str {
        if self.page_param.is_empty() {
            "page"
        } else {
            &self.page_param
        }
    }

    /// The effective window threshold (defaults to [`DEFAULT_WINDOW_THRESHOLD`]).
    pub fn window_threshold(&self) -> usize {
        self.window_threshold.unwrap_or(DEFAULT_WINDOW_THRESHOLD)
    }
}

/// The default strategy for a host. Unknown hosts default to scroll-click, the
/// most common "load more" pattern. Known infinite-scroll sites are listed
/// here so they get the right behavior out of the box.
pub fn default_strategy(host: &str) -> StrategyKind {
    match host {
        "www.parfumerie.com.ar"
        | "parfumerie.com.ar"
        | "perfumeriasrouge.com" => StrategyKind::InfiniteScroll,
        "www.juleriaque.com.ar"
        | "www.perfumeriasrouge.com" => StrategyKind::ScrollClick,
        _ => StrategyKind::ScrollClick,
    }
}

/// The effective strategy kind for `url` under `options`: an explicit
/// `options.strategy` wins, otherwise the host's registered default applies.
pub fn effective_strategy(url: &str, options: &AutoScrapeOptions) -> StrategyKind {
    let host = crate::net::host_of(url);
    options.strategy.unwrap_or_else(|| default_strategy(&host))
}

/// Whether a window reload should be triggered: count has reached the
/// threshold and the URL is a paginated `?page=N` listing.
pub fn should_window_reload(count: usize, threshold: usize, has_page_param: bool) -> bool {
    has_page_param && threshold != 0 && count >= threshold
}

/// A short display name for a strategy kind, for user-facing output.
pub fn strategy_kind_name(kind: StrategyKind) -> &'static str {
    match kind {
        StrategyKind::ScrollClick => "scroll-and-click",
        StrategyKind::InfiniteScroll => "infinite scroll",
        StrategyKind::Page => "page parameter",
    }
}

/// Common load-more selectors tried when no explicit selector is given.
pub const HEURISTIC_SELECTORS: &[&str] = &[
    "[data-role='show-more']",
    "[data-role='load-more']",
    "[data-testid='load-more']",
    "[data-testid='show-more']",
    ".load-more",
    ".loadMore",
    ".load_more",
    ".btn-load-more",
    ".show-more",
    ".showMore",
    ".pagination-next",
];

/// Common load-more button/link texts (matched case-insensitively).
pub const HEURISTIC_TEXTS: &[&str] = &[
    "cargar más",
    "cargar mas",
    "ver más",
    "ver mas",
    "load more",
    "show more",
    "mostrar más",
    "mostrar mas",
    "ver todos",
];

/// Whether `text` matches a known load-more phrase (case-insensitively).
pub fn text_matches_heuristic(text: &str) -> bool {
    HEURISTIC_TEXTS
        .iter()
        .any(|t| text.to_lowercase().contains(t))
}

/// Tracks the "product count stopped increasing" termination rule.
#[derive(Debug)]
pub struct NoGrowthTracker {
    limit: usize,
    best: usize,
    rounds: usize,
}

impl NoGrowthTracker {
    /// A tracker that lets `limit` consecutive non-growing rounds pass before
    /// declaring the listing exhausted.
    pub fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            best: 0,
            rounds: 0,
        }
    }

    /// Records the latest detected product count. Returns `true` while scraping
    /// should continue — i.e. until the count has failed to grow for `limit`
    /// consecutive rounds.
    pub fn record(&mut self, count: usize) -> bool {
        if count > self.best {
            self.best = count;
            self.rounds = 0;
            true
        } else {
            self.rounds += 1;
            self.rounds < self.limit
        }
    }

    /// The number of consecutive non-growing rounds so far.
    pub fn rounds(&self) -> usize {
        self.rounds
    }

    /// Resets the tracker to its initial state (no best, no stalled rounds).
    /// Used after a window reload drops earlier products from the DOM.
    pub fn reset(&mut self) {
        self.best = 0;
        self.rounds = 0;
    }

    /// Resets the tracker with `count` as the current best (no stalled rounds).
    /// Used after a window reload replaces the best grid with a smaller window.
    pub fn reset_to(&mut self, count: usize) {
        self.best = count;
        self.rounds = 0;
    }
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;

    #[test]
    fn text_matches_heuristic_recognizes_load_more_phrases() {
        assert!(text_matches_heuristic("Mostrar más"));
        assert!(text_matches_heuristic("MOSTRAR MÁS"));
        assert!(text_matches_heuristic("Ver más productos"));
        assert!(text_matches_heuristic("Load more"));
    }

    #[test]
    fn text_matches_heuristic_rejects_unrelated_text() {
        assert!(!text_matches_heuristic("Añadir al carrito"));
        assert!(!text_matches_heuristic(""));
        assert!(!text_matches_heuristic("Comprar ahora"));
    }

    #[test]
    fn no_growth_tracker_stops_after_limit_stalls() {
        let mut tracker = NoGrowthTracker::new(3);
        assert!(tracker.record(30));
        assert!(tracker.record(60));
        assert!(tracker.record(60));
        assert!(tracker.record(60));
        assert!(!tracker.record(60));
    }

    #[test]
    fn no_growth_tracker_resets_on_growth() {
        let mut tracker = NoGrowthTracker::new(2);
        assert!(tracker.record(10));
        assert!(tracker.record(10));
        assert!(tracker.record(20));
        assert!(tracker.record(20));
        assert!(!tracker.record(20));
    }

    #[test]
    fn default_strategy_known_infinite_scroll_host() {
        assert_eq!(
            default_strategy("www.parfumerie.com.ar"),
            StrategyKind::InfiniteScroll
        );
        assert_eq!(
            default_strategy("www.beauty24.com.ar"),
            StrategyKind::ScrollClick
        );
        assert_eq!(default_strategy(""), StrategyKind::ScrollClick);
    }

    #[test]
    fn options_default_page_param_is_page() {
        assert_eq!(AutoScrapeOptions::default().page_param_name(), "page");
        let options = AutoScrapeOptions {
            page_param: "pg".to_string(),
            ..AutoScrapeOptions::default()
        };
        assert_eq!(options.page_param_name(), "pg");
    }

    #[test]
    fn effective_strategy_respects_host_and_override() {
        let url = "https://www.parfumerie.com.ar/fragancias";
        assert_eq!(
            effective_strategy(url, &AutoScrapeOptions::default()),
            StrategyKind::InfiniteScroll
        );
        let opts = AutoScrapeOptions {
            url: url.to_string(),
            strategy: Some(StrategyKind::Page),
            page_param: "pg".to_string(),
            ..AutoScrapeOptions::default()
        };
        assert_eq!(effective_strategy(url, &opts), StrategyKind::Page);
    }

    #[test]
    fn should_window_reload_respects_threshold_and_page() {
        assert!(should_window_reload(120, 120, true));
        assert!(should_window_reload(200, 120, true));
        assert!(!should_window_reload(119, 120, true));
        assert!(!should_window_reload(120, 120, false));
        assert!(!should_window_reload(120, 0, true));
        assert!(!should_window_reload(0, 120, true));
    }

    #[test]
    fn default_window_threshold_is_120() {
        assert_eq!(
            AutoScrapeOptions::default().window_threshold(),
            DEFAULT_WINDOW_THRESHOLD
        );
        assert_eq!(
            AutoScrapeOptions {
                window_threshold: Some(200),
                ..AutoScrapeOptions::default()
            }
            .window_threshold(),
            200
        );
        assert_eq!(
            AutoScrapeOptions {
                window_threshold: Some(0),
                ..AutoScrapeOptions::default()
            }
            .window_threshold(),
            0
        );
    }
}
