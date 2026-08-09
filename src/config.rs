//! Runtime configuration loaded from `$XDG_CONFIG_HOME/price_hunter/config.toml`
//! (default `~/.config/price_hunter/config.toml`).
//!
//! Precedence: `POCKETBASE_*` env vars > `config.toml` > built-in defaults.
//! On first run `Config::ensure_template()` writes a commented template so the
//! file exists before the store tries to read it.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

const CONFIG_DIR: &str = "price_hunter";
const CONFIG_FILE: &str = "config.toml";

const TEMPLATE: &str = r#"# price_hunter configuration. Fill in the password and
# save. The template is written automatically on first run.
#
# Precedence: the POCKETBASE_URL / POCKETBASE_SUPERUSER_EMAIL /
# POCKETBASE_SUPERUSER_PASSWORD env vars override the values below, which in
# turn override the built-in defaults.

[pocketbase]
url = "http://127.0.0.1:8090"
email = "admin@pricehunter.local"
# Required: the superuser password created by setup_pocketbase.sh.
# password = "change-me"
"#;

/// All runtime settings. Unknown keys in the file are ignored; missing keys
/// fall back to `Default`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub pocketbase: Pocketbase,
}

/// PocketBase connection settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Pocketbase {
    pub url: String,
    pub email: String,
    pub password: Option<String>,
}

impl Default for Pocketbase {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1:8090".to_string(),
            email: "admin@pricehunter.local".to_string(),
            password: None,
        }
    }
}

impl Config {
    /// Path to `$XDG_CONFIG_HOME/price_hunter/config.toml` (falls back to
    /// `~/.config/...` when `XDG_CONFIG_HOME` is unset).
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_default()
            .join(CONFIG_DIR)
            .join(CONFIG_FILE)
    }

    /// Loads `config.toml`. A missing file yields defaults; a malformed file
    /// is reported as an error (never silently ignored).
    pub fn load() -> Result<Self> {
        match std::fs::read_to_string(Self::path()) {
            Ok(contents) => toml::from_str(&contents).context("could not parse config.toml"),
            Err(_) => Ok(Self::default()),
        }
    }

    /// Applies `POCKETBASE_*` env var overrides on top of the file values.
    pub fn with_env(mut self) -> Self {
        self.pocketbase.url = std::env::var("POCKETBASE_URL").unwrap_or(self.pocketbase.url);
        self.pocketbase.email =
            std::env::var("POCKETBASE_SUPERUSER_EMAIL").unwrap_or(self.pocketbase.email);
        self.pocketbase.password = std::env::var("POCKETBASE_SUPERUSER_PASSWORD")
            .ok()
            .map(Some)
            .unwrap_or(self.pocketbase.password);
        self
    }

    /// The password to authenticate with, or `None` when unset or empty.
    pub fn password(&self) -> Option<&str> {
        self.pocketbase
            .password
            .as_deref()
            .filter(|p| !p.is_empty())
    }

    /// Writes the template config to disk (creating the directory) unless the
    /// file already exists. Best-effort: a failure is logged to stderr, not
    /// fatal.
    pub fn ensure_template() {
        let path = Self::path();
        if path.exists() {
            return;
        }
        if let Err(e) = std::fs::create_dir_all(path.parent().expect("config path has a parent"))
            .and_then(|_| std::fs::write(&path, TEMPLATE))
        {
            eprintln!("could not write config template to {}: {e}", path.display());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
    }
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;

    #[test]
    fn parses_pocketbase_section() {
        let config: Config = toml::from_str(
            "[pocketbase]\nurl = \"http://pb:8090\"\nemail = \"a@b.c\"\npassword = \"s3cret\"\n",
        )
        .unwrap();
        assert_eq!(config.pocketbase.url, "http://pb:8090");
        assert_eq!(config.pocketbase.email, "a@b.c");
        assert_eq!(config.password(), Some("s3cret"));
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.pocketbase.url, "http://127.0.0.1:8090");
        assert_eq!(config.pocketbase.email, "admin@pricehunter.local");
        assert_eq!(config.password(), None);
    }

    #[test]
    fn empty_password_is_treated_as_unset() {
        let config: Config = toml::from_str("[pocketbase]\npassword = \"\"\n").unwrap();
        assert_eq!(config.password(), None);
    }

    #[test]
    fn env_vars_override_file_values() {
        let mut config: Config = toml::from_str(
            "[pocketbase]\nurl = \"http://pb:8090\"\nemail = \"a@b.c\"\npassword = \"file\"\n",
        )
        .unwrap();
        unsafe {
            std::env::set_var("POCKETBASE_URL", "http://env:8091");
            std::env::set_var("POCKETBASE_SUPERUSER_PASSWORD", "env-pass");
        }
        config = config.with_env();
        unsafe {
            std::env::remove_var("POCKETBASE_URL");
            std::env::remove_var("POCKETBASE_SUPERUSER_PASSWORD");
        }
        assert_eq!(config.pocketbase.url, "http://env:8091");
        assert_eq!(config.pocketbase.email, "a@b.c");
        assert_eq!(config.password(), Some("env-pass"));
    }
}
