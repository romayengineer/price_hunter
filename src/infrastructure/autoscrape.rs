//! Site-specific automatic scraping: drives a browser to a listing page and
//! keeps revealing more products until the detected count stops growing.
//!
//! Sites differ in how they paginate — a "load more" button, infinite scroll,
//! or `?page=N` URLs. The [`AutoScraper`] trait abstracts one such mechanism
//! per site; the shared [`scrape_until_no_growth`] loop drives any strategy to
//! completion and returns the largest product grid seen.

use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use thirtyfour::prelude::*;

use crate::domain::detect::{self, Detection};

/// Default time to wait after a load-more action for the new products to render.
pub const SETTLE: Duration = Duration::from_secs(1);

/// Default number of consecutive rounds without product-count growth before the
/// listing is declared exhausted.
pub const NO_GROWTH_LIMIT: usize = 3;

/// Default maximum number of load-more steps, as a safety valve against
/// runaway infinite scroll.
pub const MAX_STEPS: usize = 200;

/// A strategy for automatically revealing more products on a listing page.
#[async_trait]
pub trait AutoScraper {
    /// Advances the listing by one step (scroll+click, scroll, or page=N
    /// navigation). Returns `Ok(true)` if a subsequent call may reveal more
    /// products, `Ok(false)` when the strategy is exhausted (e.g. the load-more
    /// button is gone or the next page has no grid).
    async fn next(&mut self, driver: &WebDriver) -> Result<bool>;
}

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
}

/// Scrolls down and clicks a load-more button when it appears.
///
/// The page is scrolled in viewport-sized steps; after each step a load-more
/// button is searched for. With an explicit selector the button must match it;
/// otherwise a heuristic list of common classes/aria/text is used. The step
/// reports success as soon as a button is clicked, and exhaustion once the page
/// bottom is reached without finding one.
#[derive(Debug)]
pub struct ScrollAndClick {
    selector: Option<String>,
}

impl ScrollAndClick {
    /// A scroll-and-click strategy looking for the button matching `selector`
    /// (or a heuristic when `None`).
    pub fn new(selector: Option<String>) -> Self {
        Self { selector }
    }
}

#[async_trait]
impl AutoScraper for ScrollAndClick {
    async fn next(&mut self, driver: &WebDriver) -> Result<bool> {
        loop {
            if let Some(button) =
                find_load_more_button(driver, self.selector.as_deref()).await?
            {
                button.click().await?;
                return Ok(true);
            }
            if scroll_down(driver).await? {
                return Ok(false);
            }
        }
    }
}

/// Scrolls the page to the bottom on every call, letting infinite scroll load
/// more products without any button. The shared loop's count-based termination
/// decides when the listing is exhausted.
#[derive(Debug, Default)]
pub struct InfiniteScroll;

#[async_trait]
impl AutoScraper for InfiniteScroll {
    async fn next(&mut self, driver: &WebDriver) -> Result<bool> {
        let _ = driver
            .execute(
                "window.scrollTo(0, document.body.scrollHeight)",
                Vec::<serde_json::Value>::new(),
            )
            .await?;
        Ok(true)
    }
}

/// Navigates `?page=N` for increasing N, starting at page 2 (the caller already
/// loaded page 1). Reports exhaustion when a page has no product grid.
#[derive(Debug)]
pub struct PageParam {
    base_url: String,
    param: String,
    page: u32,
}

impl PageParam {
    /// A pagination strategy that visits `base_url?param=N` for N = 2, 3, ...
    pub fn new(base_url: String, param: String) -> Self {
        Self {
            base_url,
            param,
            page: 1,
        }
    }
}

#[async_trait]
impl AutoScraper for PageParam {
    async fn next(&mut self, driver: &WebDriver) -> Result<bool> {
        self.page += 1;
        driver.goto(&page_url(&self.base_url, &self.param, self.page)).await?;
        let source = driver.source().await?;
        Ok(detect::detect_grid(&source).is_some())
    }
}

/// Builds `base?param=N`, preserving any existing query string.
fn page_url(base: &str, param: &str, page: u32) -> String {
    let mut url = url::Url::parse(base).expect("base URL is valid");
    url.query_pairs_mut().append_pair(param, &page.to_string());
    url.to_string()
}

/// The default strategy for a host. Unknown hosts default to scroll-click, the
/// most common "load more" pattern. Known infinite-scroll sites are listed
/// here so they get the right behavior out of the box.
pub fn default_strategy(host: &str) -> StrategyKind {
    match host {
        "www.parfumerie.com.ar" | "parfumerie.com.ar" => StrategyKind::InfiniteScroll,
        _ => StrategyKind::ScrollClick,
    }
}

/// Builds the strategy for `url`, applying CLI overrides on top of the
/// host-specific default.
pub fn strategy_for(url: &str, options: &AutoScrapeOptions) -> Box<dyn AutoScraper> {
    match effective_strategy(url, options) {
        StrategyKind::ScrollClick => Box::new(ScrollAndClick::new(options.button.clone())),
        StrategyKind::InfiniteScroll => Box::new(InfiniteScroll),
        StrategyKind::Page => {
            Box::new(PageParam::new(url.to_string(), options.page_param_name().to_string()))
        }
    }
}

/// The effective strategy kind for `url` under `options`: an explicit
/// `options.strategy` wins, otherwise the host's registered default applies.
pub fn effective_strategy(url: &str, options: &AutoScrapeOptions) -> StrategyKind {
    let host = crate::infrastructure::util::host_of(url);
    options.strategy.unwrap_or_else(|| default_strategy(&host))
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
}

/// Drives `strategy` over the page `driver` currently shows, repeating the
/// load-more action until the detected product count stops increasing, the
/// strategy reports exhaustion, or the step budget is spent. Returns the
/// largest grid detected (or `None` if no grid was ever found).
pub async fn scrape_until_no_growth(
    driver: &WebDriver,
    strategy: &mut dyn AutoScraper,
    settle: Duration,
    max_steps: usize,
) -> Result<Option<Detection>> {
    let mut tracker = NoGrowthTracker::new(NO_GROWTH_LIMIT);
    let mut best: Option<Detection> = None;
    for _ in 0..max_steps {
        if !scrape_step(driver, strategy, &mut best, &mut tracker).await? {
            break;
        }
        tokio::time::sleep(settle).await;
    }
    Ok(best)
}

/// One iteration of the auto-scrape loop: record the current product count and
/// advance the strategy. Returns whether scraping should continue.
async fn scrape_step(
    driver: &WebDriver,
    strategy: &mut dyn AutoScraper,
    best: &mut Option<Detection>,
    tracker: &mut NoGrowthTracker,
) -> Result<bool> {
    if !record_current_products(driver, best, tracker).await? {
        return Ok(false);
    }
    strategy.next(driver).await
}

/// Detects products on the current page, updating `best` with any larger grid
/// and `tracker` with the count. Returns whether scraping should continue
/// (per the tracker's no-growth rule).
async fn record_current_products(
    driver: &WebDriver,
    best: &mut Option<Detection>,
    tracker: &mut NoGrowthTracker,
) -> Result<bool> {
    let source = driver.source().await?;
    let Some(detection) = detect::detect_grid(&source) else {
        return Ok(tracker.record(0));
    };
    let count = detection.products.len();
    if best.as_ref().is_none_or(|b| count > b.products.len()) {
        *best = Some(detection);
    }
    Ok(tracker.record(count))
}

/// Common load-more selectors tried when no explicit selector is given.
const HEURISTIC_SELECTORS: &[&str] = &[
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
const HEURISTIC_TEXTS: &[&str] = &[
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

/// Finds a visible load-more button, either matching `selector` or (when no
/// selector is given) one of the heuristic selectors/texts.
async fn find_load_more_button(
    driver: &WebDriver,
    selector: Option<&str>,
) -> Result<Option<WebElement>> {
    if let Some(selector) = selector {
        return Ok(driver.find(By::Css(selector)).await.ok());
    }
    find_by_heuristic_selector(driver).await.or(Ok(find_by_heuristic_text(driver).await))
}

/// Finds the first visible element matching a heuristic load-more selector.
async fn find_by_heuristic_selector(driver: &WebDriver) -> Result<Option<WebElement>> {
    for candidate in HEURISTIC_SELECTORS {
        if let Ok(element) = driver.find(By::Css(*candidate)).await
            && element.is_displayed().await.unwrap_or(false)
        {
            return Ok(Some(element));
        }
    }
    Ok(None)
}

/// Finds the first visible button/link whose text matches a load-more phrase.
async fn find_by_heuristic_text(driver: &WebDriver) -> Option<WebElement> {
    let elements = driver.find_all(By::Css("button, a")).await.ok()?;
    for element in elements {
        if is_visible_load_more(&element).await {
            return Some(element);
        }
    }
    None
}

/// Whether the element is a visible element whose text matches a load-more
/// phrase.
async fn is_visible_load_more(element: &WebElement) -> bool {
    matches_heuristic_text(element).await && element.is_displayed().await.unwrap_or(false)
}

/// Whether the element's text matches a known load-more phrase.
async fn matches_heuristic_text(element: &WebElement) -> bool {
    let Some(text) = element.text().await.ok() else {
        return false;
    };
    HEURISTIC_TEXTS.iter().any(|t| text.to_lowercase().contains(t))
}

/// Scrolls the page down by one viewport height. Returns `true` when the scroll
/// position did not move, meaning the page bottom was already reached.
async fn scroll_down(driver: &WebDriver) -> Result<bool> {
    let before = scroll_y(driver).await?;
    let _ = driver
        .execute(
            "window.scrollBy(0, window.innerHeight)",
            Vec::<serde_json::Value>::new(),
        )
        .await?;
    let after = scroll_y(driver).await?;
    Ok((after - before).abs() < 1.0)
}

/// The current vertical scroll offset of the page.
async fn scroll_y(driver: &WebDriver) -> Result<f64> {
    let value = driver
        .execute("return window.scrollY", Vec::<serde_json::Value>::new())
        .await?;
    Ok(value.convert::<f64>().unwrap_or(0.0))
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;

    #[test]
    fn page_url_appends_param_preserving_query() {
        assert_eq!(
            page_url("https://example.com/perfumeria", "page", 2),
            "https://example.com/perfumeria?page=2"
        );
        assert_eq!(
            page_url("https://example.com/list?sort=price", "page", 3),
            "https://example.com/list?sort=price&page=3"
        );
    }

    #[test]
    fn no_growth_tracker_stops_after_limit_stalls() {
        let mut tracker = NoGrowthTracker::new(3);
        assert!(tracker.record(30));
        assert!(tracker.record(60));
        assert!(tracker.record(60)); // stall 1
        assert!(tracker.record(60)); // stall 2
        assert!(!tracker.record(60)); // stall 3 -> stop
    }

    #[test]
    fn no_growth_tracker_resets_on_growth() {
        let mut tracker = NoGrowthTracker::new(2);
        assert!(tracker.record(10));
        assert!(tracker.record(10)); // stall 1
        assert!(tracker.record(20)); // growth resets stalls
        assert!(tracker.record(20)); // stall 1 again
        assert!(!tracker.record(20)); // stop
    }

    #[test]
    fn default_strategy_known_infinite_scroll_host() {
        assert_eq!(default_strategy("www.parfumerie.com.ar"), StrategyKind::InfiniteScroll);
        assert_eq!(default_strategy("www.beauty24.com.ar"), StrategyKind::ScrollClick);
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
    fn strategy_for_respects_host_and_override() {
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
}
