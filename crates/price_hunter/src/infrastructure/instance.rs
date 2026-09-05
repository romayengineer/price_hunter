//! Single-instance guard backed by a PID file.
//!
//! The binary launches a persistent Chrome session keyed on the project's
//! `profiles/chrome` dir, so a second `cargo run` while one is already open
//! fails ("Chrome instance exited") because the profile is locked. To avoid
//! that, the browser session takes a single-instance lock: the PID file at
//! `~/.config/price_hunter/price_hunter.pid` records the live process, and a
//! new run kills the previous one (plus any orphaned Chrome/chromedriver)
//! before launching its own browser.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};

/// Default base config dir (`~/.config`).
fn default_config_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".config")
}

/// Location of the PID file. Honors `$XDG_CONFIG_HOME` when set, otherwise
/// defaults to `~/.config/price_hunter/price_hunter.pid`.
pub fn pid_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_config_dir);
    base.join("price_hunter").join("price_hunter.pid")
}

/// Holds the single-instance lock for the browser session. Dropping it (when
/// `main` returns after the window is closed) releases the lock by removing
/// the PID file.
#[derive(Debug)]
pub struct InstanceGuard {
    path: PathBuf,
    pid: u32,
}

impl InstanceGuard {
    /// Takes the single-instance lock: kills any previously running
    /// instance (and orphaned browser processes), then writes our PID to
    /// the PID file. Creates the config directory when missing.
    pub fn acquire() -> Result<Self> {
        let path = pid_path();
        kill_previous(&path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        let pid = std::process::id();
        std::fs::write(&path, format!("{pid}\n"))
            .with_context(|| format!("could not write PID file {}", path.display()))?;
        Ok(Self { path, pid })
    }

    /// The PID written to the lock file.
    pub fn pid(&self) -> u32 {
        self.pid
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // Only remove the file if it still belongs to us: a newer instance
        // may have already taken over the lock.
        if let Ok(contents) = std::fs::read_to_string(&self.path)
            && contents.trim() == self.pid.to_string()
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Kills the process recorded in the PID file (if it is still alive and is a
/// `pricehunter` process) together with any orphaned Chrome/chromedriver that
/// could be holding the profile lock. Best-effort: failures are ignored.
fn kill_previous(pid_file: &std::path::Path) {
    let Some(pid) = read_pid(pid_file) else {
        return;
    };
    if process_alive(pid) && is_pricehunter(pid) {
        kill(pid, "TERM");
        std::thread::sleep(Duration::from_millis(1500));
        if process_alive(pid) {
            kill(pid, "KILL");
        }
    }
    // Even when the recorded PID is gone (a hard kill or Ctrl+C), a leftover
    // Chrome/chromedriver may still be holding `profiles/chrome`. Clean those
    // up and wait for the profile lock to be released so the next launch does
    // not fail with "Chrome instance exited".
    pkill_profile_browser();
    wait_for_profile_release();
}

/// Reads and parses the PID from the lock file.
fn read_pid(pid_file: &std::path::Path) -> Option<u32> {
    let contents = std::fs::read_to_string(pid_file).ok()?;
    contents.trim().parse::<u32>().ok()
}

/// Whether a process with `pid` currently exists (Unix `kill -0`).
fn process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Whether the process with `pid` is a `pricehunter` binary (guards against
/// killing an unrelated process that reused the PID).
fn is_pricehunter(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .is_some_and(|comm| {
            comm == "pricehunter" || comm.contains("price_hunter") || comm.ends_with("pricehunter")
        })
}

/// Sends `signal` ("TERM"/"KILL") to `pid`.
fn kill(pid: u32, signal: &str) {
    let _ = std::process::Command::new("kill")
        .args([format!("-{signal}"), pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status();
}

/// Kills leftover Chrome processes using the project profile dir and any
/// chromedriver launched by thirtyfour. Best-effort and scoped enough not to
/// touch unrelated browsers.
fn pkill_profile_browser() {
    let profile = PathBuf::from("profiles").join("chrome");
    let _ = std::process::Command::new("pkill")
        .args(["-f", profile.to_str().unwrap_or("profiles/chrome")])
        .stderr(std::process::Stdio::null())
        .status();
    let _ = std::process::Command::new("pkill")
        .args(["-f", "thirtyfour/drivers/chromedriver"])
        .stderr(std::process::Stdio::null())
        .status();
}

/// Polls until no process is holding the project profile dir anymore (up to
/// ~5s), giving a freshly killed Chrome time to release its lock.
fn wait_for_profile_release() {
    let profile = PathBuf::from("profiles").join("chrome");
    let pattern = profile.to_str().unwrap_or("profiles/chrome");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let held = std::process::Command::new("pgrep")
            .args(["-f", pattern])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !held {
            return;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes the two env-mutating tests: they read/write the process-global
    /// `XDG_CONFIG_HOME` and would race when the harness runs them in parallel.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn pid_path_defaults_to_dot_config() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
        let home = dirs::home_dir().unwrap();
        assert_eq!(
            pid_path(),
            home.join(".config")
                .join("price_hunter")
                .join("price_hunter.pid")
        );
    }

    #[test]
    fn pid_path_honors_xdg_config_home() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", "/tmp/ph-config");
        }
        assert_eq!(
            pid_path(),
            PathBuf::from("/tmp/ph-config")
                .join("price_hunter")
                .join("price_hunter.pid")
        );
        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
    }

    #[test]
    fn our_own_pid_is_alive_and_a_pricehunter() {
        let pid = std::process::id();
        assert!(process_alive(pid));
        assert!(is_pricehunter(pid));
    }

    #[test]
    fn impossible_pid_is_not_alive() {
        assert!(!process_alive(u32::MAX));
    }

    #[test]
    fn acquire_writes_and_drop_releases_lock() {
        // Fresh path: ensure no leftover file from a previous test run.
        let path = pid_path();
        let _ = std::fs::remove_file(&path);
        let guard = InstanceGuard::acquire().expect("acquire should succeed");
        let pid = guard.pid();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().trim(),
            pid.to_string()
        );
        drop(guard);
        assert!(
            !path.exists(),
            "PID file should be removed when the guard is dropped"
        );
        let _ = std::fs::remove_file(&path);
    }
}
