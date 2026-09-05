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
