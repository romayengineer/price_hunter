//! Time helpers shared across layers (chrono is a pure library, not I/O).

/// Formats a unix-seconds timestamp as the ISO-8601 string used for PocketBase
/// `date` fields and the matrix `generated_at` (`YYYY-MM-DD HH:MM:SS.mmmZ`,
/// UTC). The store never sends raw epoch numbers — PocketBase treats them as
/// blank.
pub fn iso8601(secs: u64) -> String {
    let dt = chrono::DateTime::from_timestamp(secs as i64, 0).unwrap_or_default();
    dt.format("%Y-%m-%d %H:%M:%S%.3fZ").to_string()
}

/// Current time as unix seconds.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_formats_utc_datetime() {
        assert_eq!(iso8601(0), "1970-01-01 00:00:00.000Z");
        assert_eq!(iso8601(1_234_567_890), "2009-02-13 23:31:30.000Z");
        assert_eq!(iso8601(123456), "1970-01-02 10:17:36.000Z");
    }
}
