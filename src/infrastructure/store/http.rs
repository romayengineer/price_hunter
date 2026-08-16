pub(super) fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Escapes a value for use inside a PocketBase filter string literal. Single
/// quotes and backslashes must be backslash-escaped or the filter parses
/// wrong (e.g. `name='A Drop d'Issey...'` → HTTP 400), which used to
/// abort the whole save and silently drop the rest of a capture.
pub(super) fn escape_filter(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
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
    fn escape_filter_handles_apostrophes_and_backslashes() {
        assert_eq!(escape_filter("plain"), "plain");
        assert_eq!(escape_filter("A Drop d'Issey"), "A Drop d\\'Issey");
        assert_eq!(escape_filter(r"a\b"), r"a\\b");
        assert_eq!(escape_filter(r"back\'slash"), r"back\\\'slash");
    }
}
