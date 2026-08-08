use rusqlite::{params, Connection, Transaction};
use url::Url;

use crate::detect::{Detection, Product};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS captures (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  url TEXT NOT NULL,
  host TEXT NOT NULL,
  captured_at INTEGER NOT NULL,
  container_classes TEXT NOT NULL,
  container_id TEXT,
  child_count INTEGER NOT NULL,
  detected_cards INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS products (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  capture_id INTEGER NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  price_text TEXT NOT NULL,
  price REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_products_capture ON products(capture_id);
CREATE INDEX IF NOT EXISTS idx_captures_host ON captures(host);
"#;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Opens (or creates) the SQLite database that TrailBase serves.
    ///
    /// TrailBase keeps its databases under `<traildepot>/data/<name>.db`. The
    /// scraper writes directly to that file (TrailBase supports existing
    /// datasets), and the TrailBase server exposes it via the admin dashboard
    /// and Record APIs when `trail run` is up.
    pub fn open_trailbase(path: &str) -> rusqlite::Result<Self> {
        Self::open(path)
    }

    /// Persists one detection (a capture + its products) in a transaction.
    pub fn save(&mut self, url: &str, captured_at: u64, detection: &Detection) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        insert_capture(&tx, url, captured_at, detection)?;
        tx.commit()
    }
}

fn insert_capture(
    tx: &Transaction,
    url: &str,
    captured_at: u64,
    detection: &Detection,
) -> rusqlite::Result<()> {
    let host = host_of(url);
    let classes = serde_json::to_string(&detection.container.classes).unwrap_or_default();
    let _ = tx.execute(
        "INSERT INTO captures
           (url, host, captured_at, container_classes, container_id, child_count, detected_cards)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            url,
            host,
            captured_at as i64,
            classes,
            detection.container.id,
            detection.container.child_count as i64,
            detection.products.len() as i64,
        ],
    )?;
    let capture_id = tx.last_insert_rowid();
    for product in &detection.products {
        insert_product(tx, capture_id, product)?;
    }
    Ok(())
}

fn insert_product(tx: &Transaction, capture_id: i64, product: &Product) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO products (capture_id, name, price_text, price)
         VALUES (?1, ?2, ?3, ?4)",
        params![capture_id, product.name, product.price_text, product.price],
    )?;
    Ok(())
}

fn host_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use super::*;
    use crate::detect::Container;

    fn sample_detection() -> Detection {
        Detection {
            container: Container {
                classes: vec!["products".to_string(), "row".to_string()],
                id: Some("grid-1".to_string()),
                child_count: 2,
            },
            products: vec![
                Product {
                    name: "Light Blue Homme EDP 50".to_string(),
                    price_text: "242.100".to_string(),
                    price: 242100.0,
                },
                Product {
                    name: "212 Vip EDP 80".to_string(),
                    price_text: "278.100".to_string(),
                    price: 278100.0,
                },
            ],
        }
    }

    fn count(store: &Store, table: &str) -> i64 {
        store
            .conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn open_creates_schema_and_in_memory() {
        let store = Store::open(":memory:").expect("in-memory db");
        assert_eq!(count(&store, "captures"), 0);
        assert_eq!(count(&store, "products"), 0);
    }

    #[test]
    fn save_persists_capture_and_products() {
        let mut store = Store::open(":memory:").expect("in-memory db");
        let url = "https://www.parfumerie.com.ar/fragancias";
        store.save(url, 123456, &sample_detection()).expect("save");
        assert_stored_capture(&store, url);
    }

    fn assert_stored_capture(store: &Store, url: &str) {
        assert_eq!(count(store, "captures"), 1);
        assert_eq!(count(store, "products"), 2);
        let (stored_url, stored_host, stored_at, classes, container_id, cards) = stored_capture(store);
        assert_eq!(
            (stored_url, stored_host, stored_at, classes, container_id.as_deref(), cards),
            (
                url.to_string(),
                "www.parfumerie.com.ar".to_string(),
                123456,
                r#"["products","row"]"#.to_string(),
                Some("grid-1"),
                2,
            )
        );
    }

    fn stored_capture(
        store: &Store,
    ) -> (String, String, i64, String, Option<String>, i64) {
        store
            .conn
            .query_row(
                "SELECT url, host, captured_at, container_classes, container_id, detected_cards
                 FROM captures",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .unwrap()
    }

    #[test]
    fn multiple_saves_append_rows() {
        let mut store = Store::open(":memory:").expect("in-memory db");
        let detection = sample_detection();
        store.save("https://a.com/x", 1, &detection).expect("save 1");
        store.save("https://b.com/y", 2, &detection).expect("save 2");
        assert_eq!(count(&store, "captures"), 2);
        assert_eq!(count(&store, "products"), 4);
    }

    #[test]
    fn empty_products_saves_zero_cards() {
        let mut store = Store::open(":memory:").expect("in-memory db");
        let mut detection = sample_detection();
        detection.products.clear();
        store.save("https://a.com/x", 1, &detection).expect("save");
        assert_eq!(count(&store, "captures"), 1);
        assert_eq!(count(&store, "products"), 0);
    }

    #[test]
    fn price_round_trips() {
        let mut store = Store::open(":memory:").expect("in-memory db");
        store
            .save("https://a.com/x", 1, &sample_detection())
            .expect("save");
        let rows: Vec<(String, String, f64)> = store
            .conn
            .prepare("SELECT name, price_text, price FROM products ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], ("Light Blue Homme EDP 50".to_string(), "242.100".to_string(), 242100.0));
    }
}
