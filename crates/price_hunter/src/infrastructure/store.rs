//! PocketBase adapter: persists detections through the Record API.

use pocketbase_sdk::client::{Auth, Client};

mod error;
pub(crate) mod http;
mod import;

pub(crate) mod repo;
mod scrape;
pub(crate) mod types;

#[cfg(test)]
mod tests;

pub use error::Error;

/// Persists detections to a running PocketBase through its Record API using the
/// normalized schema documented in DATABASE.md (providers, scrapes,
/// provider_products, provider_product_images, provider_product_prices).
///
/// The scraper NEVER writes SQL or touches the database file directly — all
/// writes go through the PocketBase HTTP API as an authenticated superuser
/// (superusers bypass collection rules, so no app user is required).
pub struct Store {
    client: Client<Auth>,
    /// Pooled HTTP client used for hot paths (e.g. bulk match inserts) where
    /// the SDK's one-shot `ureq::get/post` calls would open a fresh TCP
    /// connection per request and exhaust macOS ephemeral ports.
    agent: ureq::Agent,
}

impl Store {
    /// Authenticates against a running PocketBase instance.
    ///
    /// Settings come from `~/.config/price_hunter/config.toml` (see
    /// `config::Config`) with `POCKETBASE_URL`, `POCKETBASE_SUPERUSER_EMAIL`
    /// and `POCKETBASE_SUPERUSER_PASSWORD` env vars overriding the file. The
    /// password is required (file or env).
    pub fn connect() -> Result<Self, Error> {
        let config = crate::infrastructure::config::Config::load()
            .map_err(|e| Error::Config(format!("{e:#}")))?;
        let config = config.with_env();
        let password = config.password().map(str::to_owned).ok_or_else(|| {
            Error::Config(format!(
                "no PocketBase password configured — set the password in {} or export \
                 POCKETBASE_SUPERUSER_PASSWORD",
                crate::infrastructure::config::Config::path().display()
            ))
        })?;
        let base_url = config.pocketbase.url;
        let email = config.pocketbase.email;
        let client = Client::new(&base_url)
            .superusers()
            .auth_with_password(&email, &password)
            .map_err(|e| Error::Auth(format!("could not authenticate at {base_url}: {e}")))?;
        Ok(Self {
            client,
            agent: ureq::Agent::new(),
        })
    }
}
