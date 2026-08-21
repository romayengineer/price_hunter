//! Site-specific automatic scraping: drives a browser to a listing page and
//! keeps revealing more products until the detected count stops growing.
//!
//! Sites differ in how they paginate — a "load more" button, infinite scroll,
//! or `?page=N` URLs. The [`AutoScraper`] trait abstracts one such mechanism
//! per site; the shared [`scrape_until_no_growth`] loop drives any strategy to
//! completion and returns the largest product grid seen.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use thirtyfour::prelude::*;

/// Minimal driver abstraction so `WindowedAutoScraper` can be tested offline
/// with a fake driver that serves synthetic HTML and records `goto` calls.
#[async_trait]
pub trait DriverPage: Send + Sync {
    /// The current page URL.
    async fn current_url(&self) -> Result<String>;
    /// The current page HTML source.
    async fn source(&self) -> Result<String>;
    /// Navigates to `url`.
    async fn goto(&self, url: &str) -> Result<()>;
}

#[async_trait]
impl DriverPage for WebDriver {
    async fn current_url(&self) -> Result<String> {
        Ok(self.current_url().await?.to_string())
    }
    async fn source(&self) -> Result<String> {
        Ok(self.source().await?)
    }
    async fn goto(&self, url: &str) -> Result<()> {
        Ok(self.goto(url).await?)
    }
}

use crate::domain::detect::{self, Detection};

static WINDOW_RELOADED: AtomicBool = AtomicBool::new(false);

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
pub const DEFAULT_WINDOW_THRESHOLD: usize = 120;

/// A strategy for automatically revealing more products on a listing page.
#[async_trait]
pub trait AutoScraper: Send + Sync {
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
                log::info!("scroll-click: found load-more button, clicking");
                click_load_more(driver, &button).await?;
                return Ok(true);
            }
            if scroll_down(driver).await? {
                log::info!("scroll-click: page bottom reached, no load-more button found");
                return Ok(false);
            }
        }
    }
}

/// Clicks a load-more button robustly: scrolls it into view, then tries a
/// normal click, falling back to a JavaScript click when the browser reports
/// the click was intercepted (common when the button is at the very bottom and
/// a product image overlaps its hit area).
async fn click_load_more(driver: &WebDriver, button: &WebElement) -> Result<()> {
    let _ = button.scroll_into_view().await;
    if button.click().await.is_ok() {
        return Ok(());
    }
    let args = vec![button.to_json()?];
    let _ = driver.execute("arguments[0].click()", args).await;
    Ok(())
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
        let url = page_url(&self.base_url, &self.param, self.page);
        log::info!("page strategy: navigating to page {} ({url})", self.page);
        driver.goto(&url).await?;
        let source = driver.source().await?;
        let has_grid = detect::detect_grid(&source).is_some();
        if !has_grid {
            log::info!("page strategy: page {} has no grid, stopping", self.page);
        }
        Ok(has_grid)
    }
}

/// Builds `base?param=N`, preserving any existing query string. If `param`
/// already exists, its value is replaced (not duplicated).
fn page_url(base: &str, param: &str, page: u32) -> String {
    set_page_url(base, param, page)
}

/// Sets `param=N` in `base`, preserving other query pairs. Replaces any
/// existing `param` value instead of appending a duplicate.
pub fn set_page_url(base: &str, param: &str, page: u32) -> String {
    let mut url = url::Url::parse(base).expect("base URL is valid");
    let other: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| k != param)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    url.query_pairs_mut().clear();
    for (k, v) in other {
        url.query_pairs_mut().append_pair(&k, &v);
    }
    url.query_pairs_mut()
        .append_pair(param, &page.to_string());
    url.to_string()
}

/// Whether `url`'s query string contains `param`.
pub fn url_contains_page_param(url: &str, param: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    parsed.query_pairs().any(|(k, _)| k == param)
}

/// The last value of `param` in `url`'s query string, parsed as `u32`.
pub fn extract_page(url: &str, param: &str) -> Option<u32> {
    let parsed = url::Url::parse(url).ok()?;
    let mut last = None;
    for (k, v) in parsed.query_pairs() {
        if k == param && let Ok(n) = v.parse::<u32>() {
            last = Some(n);
        }
    }
    last
}

/// Whether a window reload should be triggered: count has reached the
/// threshold and the URL is a paginated `?page=N` listing.
pub fn should_window_reload(count: usize, threshold: usize, has_page_param: bool) -> bool {
    has_page_param && threshold != 0 && count >= threshold
}

/// Wraps another [`AutoScraper`] and reloads the same `?page=N` when the
/// detected product count reaches `threshold`. This drops earlier products from
/// the DOM (they remain in the caller's `seen` set via incremental saves) so
/// the page stays responsive on large listings. Only active when the current
/// URL contains `param`.
pub struct WindowedAutoScraper {
    inner: Box<dyn AutoScraper>,
    param: String,
    threshold: usize,
    reloaded: HashSet<u32>,
}

impl WindowedAutoScraper {
    /// Wraps `inner` with windowing for `param` at `threshold`.
    pub fn new(inner: Box<dyn AutoScraper>, param: String, threshold: usize) -> Self {
        Self {
            inner,
            param,
            threshold,
            reloaded: HashSet::new(),
        }
    }
}

#[async_trait]
impl AutoScraper for WindowedAutoScraper {
    async fn next(&mut self, driver: &WebDriver) -> Result<bool> {
        // Pre-click check: if DOM already large before advancing, reload immediately.
        if self.try_reload_if_needed(driver).await? {
            return Ok(true);
        }
        let progressed = self.inner.next(driver).await?;
        if !progressed {
            return Ok(false);
        }
        // Post-growth check: immediately after the site action (live DOM total),
        // so a batch that crosses 120 (e.g. 108 -> 126) triggers reload in the same step
        // instead of one batch later. Brief sleep lets the page settle before peeking.
        tokio::time::sleep(POLL_INTERVAL).await;
        if self.try_reload_if_needed(driver).await? {
            return Ok(true);
        }
        Ok(true)
    }
}

impl WindowedAutoScraper {
    #[allow(clippy::cognitive_complexity)]
    async fn try_reload_if_needed<D: DriverPage>(&mut self, driver: &D) -> Result<bool> {
        if self.threshold == 0 {
            return Ok(false);
        }
        let current_url = driver.current_url().await.unwrap_or_default();
        if !url_contains_page_param(&current_url, &self.param) {
            return Ok(false);
        }
        let total = peek_count(driver).await?;
        if !should_window_reload(total, self.threshold, true) {
            log::debug!(
                "window: {total} products < threshold {}, continuing ({current_url})",
                self.threshold
            );
            return Ok(false);
        }
        let page = extract_page(&current_url, &self.param).unwrap_or(1);
        if self.reloaded.contains(&page) {
            log::debug!(
                "window: page {page} already reloaded, skipping same-url reload (total {total} >= {})",
                self.threshold
            );
            return Ok(false);
        }
        self.reloaded.insert(page);
        let url = set_page_url(&current_url, &self.param, page);
        log::info!(
            "memory optimization: reloading {url} (window threshold {} reached with {total} products)",
            self.threshold
        );
        WINDOW_RELOADED.store(true, Ordering::SeqCst);
        driver.goto(&url).await?;
        Ok(true)
    }
}

async fn peek_count<D: DriverPage>(driver: &D) -> Result<usize> {
    let url = driver.current_url().await.unwrap_or_default();
    let source = driver.source().await?;
    let count = detect::detect_grid(&source)
        .map(|d| d.products.len())
        .unwrap_or(0);
    log::debug!("window peek: {count} products on {url}");
    Ok(count)
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
/// host-specific default. Any strategy is wrapped with
/// [`WindowedAutoScraper`] when windowing is enabled (`window_threshold != 0`);
/// the wrapper itself checks `current_url` for `?page=N` (or custom
/// `-page-param`) at runtime, so the same page is reloaded once the product count
/// reaches `window_threshold`, dropping earlier products from the DOM.
pub fn strategy_for(url: &str, options: &AutoScrapeOptions) -> Box<dyn AutoScraper> {
    let inner: Box<dyn AutoScraper> = match effective_strategy(url, options) {
        StrategyKind::ScrollClick => Box::new(ScrollAndClick::new(options.button.clone())),
        StrategyKind::InfiniteScroll => Box::new(InfiniteScroll),
        StrategyKind::Page => {
            Box::new(PageParam::new(url.to_string(), options.page_param_name().to_string()))
        }
    };
    let threshold = options.window_threshold();
    let param = options.page_param_name();
    if threshold != 0 {
        Box::new(WindowedAutoScraper::new(inner, param.to_string(), threshold))
    } else {
        inner
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

    /// The number of consecutive non-growing rounds so far.
    pub fn rounds(&self) -> usize {
        self.rounds
    }
}

/// Drives `strategy` over the page `driver` currently shows, repeating the
/// load-more action until the detected product count stops increasing, the
/// strategy reports exhaustion, or the step budget is spent. `on_growth` is
/// invoked with the current (largest) detection whenever the detected product
/// count grows, so callers can persist incrementally instead of waiting for the
/// loop to finish. Returns the largest grid detected (or `None` if no grid was
/// ever found).
///
/// After each load-more action the loop waits (polling every [`POLL_INTERVAL`],
/// up to `load_timeout`) for the product count to actually grow, so one action
/// fully loads its batch before the next is issued instead of re-clicking while
/// the page is still loading.
pub async fn scrape_until_no_growth(
    driver: &WebDriver,
    strategy: &mut dyn AutoScraper,
    load_timeout: Duration,
    max_steps: usize,
    on_growth: impl FnMut(&Detection),
) -> Result<Option<Detection>> {
    let mut state = ScrapeState {
        best: None,
        tracker: NoGrowthTracker::new(NO_GROWTH_LIMIT),
        last_count: 0,
        on_growth: Box::new(on_growth),
    };

    state.last_count = detect_products(driver, &mut state).await?;
    log::info!(
        "auto-scrape initial: best = {} products (load_timeout={load_timeout:?}, max_steps={max_steps})",
        state.last_count
    );

    run_loop(driver, strategy, &mut state, load_timeout, max_steps).await?;
    Ok(state.best)
}

/// Runs the load-more loop until the strategy is exhausted, the no-growth
/// limit is reached, or `max_steps` is spent.
async fn run_loop(
    driver: &WebDriver,
    strategy: &mut dyn AutoScraper,
    state: &mut ScrapeState<'_>,
    load_timeout: Duration,
    max_steps: usize,
) -> Result<()> {
    for step in 0..max_steps {
        if !auto_scrape_iteration(driver, strategy, state, load_timeout, step).await? {
            return Ok(());
        }
    }
    log::info!("auto-scrape reached max_steps={max_steps}, stopping");
    Ok(())
}

/// Mutable state threaded through the auto-scrape loop.
struct ScrapeState<'a> {
    best: Option<Detection>,
    tracker: NoGrowthTracker,
    last_count: usize,
    on_growth: Box<dyn FnMut(&Detection) + 'a>,
}

/// One iteration of the auto-scrape loop: advances the strategy (one load-more
/// action), waits for its products to load, and records the step in the
/// no-growth tracker. Returns `false` when the loop should stop.
#[allow(clippy::cognitive_complexity)]
async fn auto_scrape_iteration(
    driver: &WebDriver,
    strategy: &mut dyn AutoScraper,
    state: &mut ScrapeState<'_>,
    load_timeout: Duration,
    step: usize,
) -> Result<bool> {
    if !strategy.next(driver).await? {
        log::info!("auto-scrape step {step}: strategy exhausted");
        return Ok(false);
    }
    let count = wait_for_growth(driver, state, state.last_count, load_timeout).await?;
    state.last_count = count;
    let keep_going = state.tracker.record(count);
    if !keep_going {
        log::info!(
            "auto-scrape step {step}: no growth for {} rounds, stopping (best = {count} products)",
            state.tracker.rounds()
        );
    } else {
        log::info!("auto-scrape step {step}: best = {count} products");
    }
    Ok(keep_going)
}

/// Detects products on the current page, updating `state.best` with any larger
/// grid and invoking `on_growth` when the count grows. Returns the current best
/// product count. When a memory-optimization window reload just happened
/// (`WINDOW_RELOADED`), the new window's grid replaces `best` even when
/// smaller, and the no-growth tracker is reset so subsequent growth in the
/// new window is not treated as a stall.
#[allow(clippy::cognitive_complexity)]
async fn detect_products(driver: &WebDriver, state: &mut ScrapeState<'_>) -> Result<usize> {
    let url = driver
        .current_url()
        .await
        .map(|u| u.to_string())
        .unwrap_or_default();
    let source = driver.source().await?;
    let Some(detection) = detect::detect_grid(&source) else {
        log::info!("no product grid on {url}");
        if WINDOW_RELOADED.swap(false, Ordering::SeqCst) {
            log::info!("window reload: resetting best after navigation to {url}");
            state.best = None;
            state.tracker.best = 0;
            state.tracker.rounds = 0;
            return Ok(0);
        }
        return Ok(count_of(&state.best));
    };
    let count = detection.products.len();
    let container = &detection.container;
    log::info!(
        "detected {count} products on {url} (container: {:?}, child_count: {})",
        container.classes,
        container.child_count
    );
    if WINDOW_RELOADED.swap(false, Ordering::SeqCst) {
        log::info!("window reload: replacing best with {count} products after navigation to {url}");
        state.best = Some(detection);
        state.tracker.best = count;
        state.tracker.rounds = 0;
        (state.on_growth)(state.best.as_ref().expect("best was just set"));
        return Ok(count);
    }
    if state.best.as_ref().is_none_or(|b| count > b.products.len()) {
        log::info!("new best: {count} products (was {})", count_of(&state.best));
        state.best = Some(detection);
        (state.on_growth)(state.best.as_ref().expect("best was just set"));
    }
    Ok(count_of(&state.best))
}

/// Polls the product count until it exceeds `before` (the count from before a
/// load-more action) or `timeout` elapses, updating `state.best` and firing
/// `on_growth` whenever the count grows. Returns the current best count.
async fn wait_for_growth(
    driver: &WebDriver,
    state: &mut ScrapeState<'_>,
    before: usize,
    timeout: Duration,
) -> Result<usize> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let count = detect_products(driver, state).await?;
        if count > before {
            log::info!("product count grew from {before} to {count}");
            return Ok(count);
        }
        if tokio::time::Instant::now() >= deadline {
            log::info!("waited {timeout:?} for product count to grow from {before}, current = {count}");
            return Ok(count);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// The number of products in the best detection so far (0 when none).
fn count_of(best: &Option<Detection>) -> usize {
    best.as_ref().map_or(0, |d| d.products.len())
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
    if let Some(element) = find_by_heuristic_selector(driver).await? {
        return Ok(Some(element));
    }
    Ok(find_by_heuristic_text(driver).await)
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
    text_matches_heuristic(&text)
}

/// Whether `text` matches a known load-more phrase (case-insensitively).
fn text_matches_heuristic(text: &str) -> bool {
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
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    static WINDOW_TEST_LOCK: StdMutex<()> = StdMutex::new(());

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

    #[test]
    fn set_page_url_replaces_existing_page() {
        assert_eq!(
            set_page_url("https://example.com/list?page=2", "page", 5),
            "https://example.com/list?page=5"
        );
        assert_eq!(
            set_page_url("https://example.com/list?page=2&sort=price", "page", 5),
            "https://example.com/list?sort=price&page=5"
        );
        assert_eq!(
            set_page_url("https://example.com/list?sort=price&page=2&foo=bar", "page", 3),
            "https://example.com/list?sort=price&foo=bar&page=3"
        );
        // page_url now delegates to set_page_url and must not duplicate
        assert_eq!(
            page_url("https://example.com/list?page=2", "page", 3),
            "https://example.com/list?page=3"
        );
    }

    #[test]
    fn url_contains_page_param_detects_query() {
        assert!(url_contains_page_param(
            "https://example.com/list?page=1",
            "page"
        ));
        assert!(url_contains_page_param(
            "https://example.com/list?sort=price&page=2",
            "page"
        ));
        assert!(!url_contains_page_param(
            "https://example.com/list?sort=price",
            "page"
        ));
        assert!(url_contains_page_param(
            "https://example.com/list?pg=2",
            "pg"
        ));
        assert!(!url_contains_page_param(
            "https://example.com/list?page=1",
            "pg"
        ));
    }

    #[test]
    fn extract_page_parses_last_value() {
        assert_eq!(
            extract_page("https://example.com/list?page=3", "page"),
            Some(3)
        );
        assert_eq!(
            extract_page("https://example.com/list?sort=price&page=5", "page"),
            Some(5)
        );
        assert_eq!(extract_page("https://example.com/list", "page"), None);
        assert_eq!(
            extract_page("https://example.com/list?page=bad", "page"),
            None
        );
        // duplicate param: last wins (set_page_url normalizes to one, but parse may see duplicates)
        assert_eq!(
            extract_page("https://example.com/list?page=2&page=5", "page"),
            Some(5)
        );
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
        assert_eq!(DEFAULT_WINDOW_THRESHOLD, 120);
        assert_eq!(AutoScrapeOptions::default().window_threshold(), 120);
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

    #[test]
    fn strategy_for_wraps_windowed_when_url_has_page_param() {
        // Non-Page strategy with ?page=N should be windowed (inner wrapped).
        // We verify indirectly via url_contains check and threshold; the wrapper
        // is applied in strategy_for but type-erased, so we test the helpers
        // that drive wrapping.
        let url = "https://example.com/list?page=3";
        let opts = AutoScrapeOptions {
            url: url.to_string(),
            ..AutoScrapeOptions::default()
        };
        assert!(url_contains_page_param(url, opts.page_param_name()));
        assert!(!matches!(
            effective_strategy(url, &opts),
            StrategyKind::Page
        ));
        // With threshold 0, should not window
        let opts_zero = AutoScrapeOptions {
            url: url.to_string(),
            window_threshold: Some(0),
            ..AutoScrapeOptions::default()
        };
        assert_eq!(opts_zero.window_threshold(), 0);
    }

    // --- Offline fake driver for window-reload tests ---

    use std::sync::Mutex;

    struct FakeDriver {
        url: Mutex<String>,
        html: String,
        goto_log: Mutex<Vec<String>>,
    }

    impl FakeDriver {
        fn with_products(url: &str, n: usize) -> Self {
            Self {
                url: Mutex::new(url.to_string()),
                html: fake_html(n),
                goto_log: Mutex::new(Vec::new()),
            }
        }
        #[allow(dead_code)]
        fn with_html(url: &str, html: String) -> Self {
            Self {
                url: Mutex::new(url.to_string()),
                html,
                goto_log: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl DriverPage for FakeDriver {
        async fn current_url(&self) -> Result<String> {
            Ok(self.url.lock().unwrap().clone())
        }
        async fn source(&self) -> Result<String> {
            Ok(self.html.clone())
        }
        async fn goto(&self, url: &str) -> Result<()> {
            *self.url.lock().unwrap() = url.to_string();
            self.goto_log.lock().unwrap().push(url.to_string());
            Ok(())
        }
    }

    struct NoopScraper;
    #[async_trait::async_trait]
    impl AutoScraper for NoopScraper {
        async fn next(&mut self, _driver: &WebDriver) -> Result<bool> {
            Ok(true)
        }
    }

    fn fake_html(n: usize) -> String {
        let mut cards = String::new();
        for i in 0..n {
            cards.push_str(&format!(
                r#"<div class="card"><a href="/p{i}">Product {i}</a><span class="price">${}.00</span></div>"#,
                10 + i
            ));
        }
        format!(
            r#"<html><body><div class="product-grid">{cards}</div></body></html>"#
        )
    }

    fn reset_window_state() {
        WINDOW_RELOADED.store(false, Ordering::SeqCst);
    }

    #[tokio::test]
    async fn window_reload_fires_at_threshold() {
        let _guard = WINDOW_TEST_LOCK.lock().unwrap();
        reset_window_state();
        let driver = FakeDriver::with_products("https://example.com/list?page=8", 120);
        let mut scraper = WindowedAutoScraper::new(Box::new(NoopScraper), "page".to_string(), 120);
        // should trigger reload via try_reload_if_needed (live DOM total >= threshold)
        let reloaded = scraper.try_reload_if_needed(&driver).await.unwrap();
        assert!(reloaded, "expected reload when 120 >= 120");
        assert_eq!(driver.goto_log.lock().unwrap().len(), 1);
        assert_eq!(driver.goto_log.lock().unwrap()[0], "https://example.com/list?page=8");
        assert!(WINDOW_RELOADED.load(Ordering::SeqCst));
        assert!(scraper.reloaded.contains(&8));
    }

    #[tokio::test]
    async fn window_no_reload_under_threshold() {
        let _guard = WINDOW_TEST_LOCK.lock().unwrap();
        reset_window_state();
        let driver = FakeDriver::with_products("https://example.com/list?page=8", 119);
        let mut scraper = WindowedAutoScraper::new(Box::new(NoopScraper), "page".to_string(), 120);
        let reloaded = scraper.try_reload_if_needed(&driver).await.unwrap();
        assert!(!reloaded, "should not reload when 119 < 120");
        assert!(driver.goto_log.lock().unwrap().is_empty());
        assert!(!WINDOW_RELOADED.load(Ordering::SeqCst));
        assert!(!scraper.reloaded.contains(&8));
    }

    #[tokio::test]
    async fn window_already_reloaded_page_skips() {
        let _guard = WINDOW_TEST_LOCK.lock().unwrap();
        reset_window_state();
        let driver = FakeDriver::with_products("https://example.com/list?page=8", 120);
        let mut scraper = WindowedAutoScraper::new(Box::new(NoopScraper), "page".to_string(), 120);
        // first reload
        assert!(scraper.try_reload_if_needed(&driver).await.unwrap());
        assert_eq!(driver.goto_log.lock().unwrap().len(), 1);
        // WINDOW_RELOADED was set, reset to simulate detect_products consuming it
        WINDOW_RELOADED.store(false, Ordering::SeqCst);
        // second call with same page should not reload again (once per page)
        let second = scraper.try_reload_if_needed(&driver).await.unwrap();
        assert!(!second, "second reload on same page should be skipped");
        assert_eq!(driver.goto_log.lock().unwrap().len(), 1, "no second goto");
    }

    #[tokio::test]
    async fn window_reload_exact_same_url_preserving_params() {
        let _guard = WINDOW_TEST_LOCK.lock().unwrap();
        reset_window_state();
        let driver = FakeDriver::with_products(
            "https://example.com/list?sort=price&page=8&foo=bar",
            120,
        );
        let mut scraper = WindowedAutoScraper::new(Box::new(NoopScraper), "page".to_string(), 120);
        let reloaded = scraper.try_reload_if_needed(&driver).await.unwrap();
        assert!(reloaded);
        // should preserve other query params and replace page param at end
        assert_eq!(
            driver.goto_log.lock().unwrap()[0],
            "https://example.com/list?sort=price&foo=bar&page=8"
        );
        assert!(WINDOW_RELOADED.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn window_no_reload_without_page_param() {
        let _guard = WINDOW_TEST_LOCK.lock().unwrap();
        reset_window_state();
        let driver = FakeDriver::with_products("https://example.com/list?sort=price", 200);
        let mut scraper = WindowedAutoScraper::new(Box::new(NoopScraper), "page".to_string(), 120);
        let reloaded = scraper.try_reload_if_needed(&driver).await.unwrap();
        assert!(!reloaded, "no page param -> no reload even if 200 >=120");
        assert!(driver.goto_log.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn window_threshold_zero_disables() {
        let _guard = WINDOW_TEST_LOCK.lock().unwrap();
        reset_window_state();
        let driver = FakeDriver::with_products("https://example.com/list?page=8", 200);
        let mut scraper = WindowedAutoScraper::new(Box::new(NoopScraper), "page".to_string(), 0);
        let reloaded = scraper.try_reload_if_needed(&driver).await.unwrap();
        assert!(!reloaded, "threshold 0 disables");
        assert!(driver.goto_log.lock().unwrap().is_empty());
    }
}
