//! Pure URL helpers: hostname extraction and `?page=N` query manipulation.
//! No I/O — only the `url` crate for parsing.

/// The hostname part of `url` (e.g. `www.example.com`), or `""` when the URL
/// is malformed or has no host.
pub fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Builds `base?param=N`, preserving any existing query string. If `param`
/// already exists, its value is replaced (not duplicated).
pub fn page_url(base: &str, param: &str, page: u32) -> String {
    set_page_url(base, param, page)
}

/// Sets `param=N` in `base`, preserving other query pairs. Replaces any
/// existing `param` value instead of appending a duplicate.
pub fn set_page_url(base: &str, param: &str, page: u32) -> String {
    let mut url = match url::Url::parse(base) {
        Ok(u) => u,
        Err(_) => {
            // Fallback for non-absolute or malformed base: simple query-string append
            // without panicking (library paths must not panic on user input).
            let sep = if base.contains('?') { "&" } else { "?" };
            return format!("{base}{sep}{param}={page}");
        }
    };
    let other: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| k != param)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    url.query_pairs_mut().clear();
    for (k, v) in other {
        url.query_pairs_mut().append_pair(&k, &v);
    }
    url.query_pairs_mut().append_pair(param, &page.to_string());
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
        if k == param
            && let Ok(n) = v.parse::<u32>()
        {
            last = Some(n);
        }
    }
    last
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;

    #[test]
    fn host_of_extracts_host() {
        assert_eq!(
            host_of("https://www.parfumerie.com.ar/fragancias"),
            "www.parfumerie.com.ar"
        );
        assert_eq!(host_of("not a url"), "");
    }

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
            set_page_url(
                "https://example.com/list?sort=price&page=2&foo=bar",
                "page",
                3
            ),
            "https://example.com/list?sort=price&foo=bar&page=3"
        );
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
        assert_eq!(
            extract_page("https://example.com/list?page=2&page=5", "page"),
            Some(5)
        );
    }
}
