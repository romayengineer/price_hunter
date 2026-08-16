/// Formats a unix-seconds timestamp as the ISO-8601 string PocketBase expects
/// for `date` fields (`YYYY-MM-DD HH:MM:SS.mmmZ`, UTC). The store never sends
/// raw epoch numbers — PocketBase treats them as blank.
pub(crate) fn iso8601(secs: u64) -> String {
    let dt = chrono::DateTime::from_timestamp(secs as i64, 0).unwrap_or_default();
    dt.format("%Y-%m-%d %H:%M:%S%.3fZ").to_string()
}

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

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

    #[test]
    fn iso8601_formats_utc_datetime() {
        assert_eq!(iso8601(0), "1970-01-01 00:00:00.000Z");
        assert_eq!(iso8601(1_234_567_890), "2009-02-13 23:31:30.000Z");
        assert_eq!(iso8601(123456), "1970-01-02 10:17:36.000Z");
    }
}
