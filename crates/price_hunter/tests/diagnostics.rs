//! Env-var-parameterized diagnostic tools (no-ops without their env var, so a
//! normal test run stays clean). Keep these — they are the debugging entry
//! points when detection picks the wrong container or a live page differs from
//! its fixture.

#[path = "diagnostics/mod.rs"]
mod diagnostics;
