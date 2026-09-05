//! Progress reporting port for the application layer. Use cases report their
//! progress through this trait instead of writing to stdout directly, so the
//! library stays side-effect-free. The binary provides a stdout implementation;
//! library and test consumers use [`NoopReporter`].

/// Receives progress updates from bulk application operations.
pub trait Reporter {
    /// Reports `done` out of `total` units completed. Called on the same
    /// thread as the work; implementors may redraw a progress line.
    fn progress(&mut self, _done: usize, _total: usize) {}
}

/// A reporter that discards every update — the default for library/test use.
pub struct NoopReporter;

impl Reporter for NoopReporter {}

/// Minimum percentage-point change before a redrawn progress line is worth
/// emitting (avoids spamming the terminal on large backfills).
pub const PROGRESS_EPSILON_PCT: f64 = 0.005;

/// The `0–100` percentage for `done` out of `total`, or `None` when `total`
/// is zero (caller should finish the line).
pub fn progress_pct(done: usize, total: usize) -> Option<f64> {
    if total == 0 {
        return None;
    }
    Some(done as f64 * 100.0 / total as f64)
}

/// Whether `pct` differs enough from the last emitted value to redraw.
pub fn should_emit_progress(last_pct: f64, pct: f64) -> bool {
    (pct - last_pct).abs() >= PROGRESS_EPSILON_PCT
}
