CREATE TABLE IF NOT EXISTS captures (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  url TEXT NOT NULL,
  host TEXT NOT NULL,
  captured_at INTEGER NOT NULL,
  container_classes TEXT NOT NULL,
  container_id TEXT,
  child_count INTEGER NOT NULL,
  detected_cards INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS products (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  capture_id INTEGER NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  price_text TEXT NOT NULL,
  price REAL NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_products_capture ON products(capture_id);
CREATE INDEX IF NOT EXISTS idx_captures_host ON captures(host);
