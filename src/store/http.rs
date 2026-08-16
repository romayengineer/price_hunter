/// Formats a unix-seconds timestamp as the ISO-8601 string PocketBase expects
/// for `date` fields (`YYYY-MM-DD HH:MM:SS.mmmZ`, UTC). The store never sends
/// raw epoch numbers — PocketBase treats them as blank.
pub(crate) fn iso8601(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}.000Z")
}

/// Gregorian calendar day to (year, month, day) from the Unix epoch in days
/// (Howard Hinnant's `civil_from_days` algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
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
