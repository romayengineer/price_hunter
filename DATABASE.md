# Database

## ER Diagram

```mermaid
erDiagram
    direction LR

    providers {
        int id PK
        varchar domain UK "e.g. www.provider.com"
        varchar name
        boolean enabled
        varchar default_currency "fallback for provider_product_prices.currency"
        datetime created_at
    }

    products {
        int id PK
        varchar brand UK "unique with name, size"
        varchar name UK "unique with brand, size"
        varchar size UK "unique with brand, name"
        varchar category
        boolean active
        datetime created_at
        datetime updated_at
    }

    scrapes {
        int id PK
        int provider_id FK "references providers.id"
        varchar url "page that was captured"
        datetime scraped_at
        varchar status "success, failed, empty"
        varchar capture_path "captures/<domain>/capture-<ts>.json"
        int product_count "detected cards"
        varchar container_class "grid container class"
    }

    provider_products {
        int id PK
        int provider_id FK "references providers.id"
        varchar provider_product_url UK "stable per-provider key"
        varchar sku "provider's own product id"
        varchar gtin_ean "optional barcode"
        varchar product_name "name extracted from HTML"
        varchar provider_brand "brand as scraped"
        varchar provider_size "size as scraped"
        varchar availability "in_stock, out_of_stock, ..."
        int product_id FK, UK "nullable, confirmed link to products.id"
        datetime last_seen_at
        datetime created_at
    }

    provider_product_images {
        int id PK
        int provider_product_id FK "references provider_products.id"
        varchar url UK "unique per provider product"
        int position "display order"
        boolean is_primary
        datetime created_at
        datetime updated_at
    }

    provider_product_matches {
        int id PK
        int provider_product_id FK "references provider_products.id"
        int product_id FK "references products.id"
        decimal score "fuzzy match confidence"
        varchar status "pending, confirmed, rejected"
        datetime created_at
        datetime updated_at
    }

    provider_product_prices {
        int id PK
        int provider_product_id FK "references provider_products.id"
        int scrape_id FK "references scrapes.id"
        decimal price
        varchar currency "e.g. ARS, USD"
        varchar price_text "raw text from HTML"
        datetime created_at UK "unique with provider_product_id"
    }

    providers ||--o{ scrapes : "scraped by"
    providers ||--o{ provider_products : "lists"
    products o|--o{ provider_products : "matched to"
    provider_products ||--o{ provider_product_matches : "has candidates"
    products o|--o{ provider_product_matches : "suggested as"
    provider_products ||--o{ provider_product_images : "shows"
    scrapes ||--o{ provider_product_prices : "observed in"
    provider_products ||--o{ provider_product_prices : "has prices at"
```

## Tables

### providers

| Column    | Type    | Notes |
| --------- | ------- | ----- |
| id         | int     | primary key |
| domain     | varchar | unique, e.g. `www.provider.com` |
| name       | varchar | display name |
| enabled    | boolean | whether this provider is being scraped |
| default_currency | varchar | fallback when no currency is detected on a card (e.g. `ARS`) |
| created_at | datetime | |

### products

Canonical, known products.

| Column     | Type     | Notes |
| ---------- | -------- | ----- |
| id         | int      | primary key |
| brand      | varchar  | |
| name       | varchar  | |
| size       | varchar  | |
| category   | varchar  | optional; improves fuzzy matching |
| active     | boolean  | `false` when retired; not eligible for new matches |
| created_at | datetime | |
| updated_at | datetime | when the row was last edited |

Unique: `(brand, name, size)` — enforced with `COALESCE` so a missing brand/size still participates in the uniqueness check.

### scrapes

One poll of a provider page (one `capture-<timestamp>.json` file).

| Column        | Type     | Notes |
| ------------- | -------- | ----- |
| id            | int      | primary key |
| provider_id   | int      | foreign key → `providers.id` |
| url           | varchar  | page that was captured |
| scraped_at    | datetime | when the poll happened |
| status        | varchar  | `success`, `failed`, `empty` |
| capture_path  | varchar  | `captures/<domain>/capture-<ts>.json` |
| product_count | int      | detected cards in the capture |
| container_class | varchar | grid container class, e.g. `vtex-search-result-3-x-gallery` |

### provider_products

A product as listed on a specific provider's site. `product_name` is extracted from the HTML; the confirmed mapping to a canonical `products` row lives in `product_id`, with candidate/recommended matches in `provider_product_matches`.

| Column                | Type     | Notes |
| --------------------- | -------- | ----- |
| id                    | int      | primary key |
| provider_id           | int      | foreign key → `providers.id` |
| provider_product_url  | varchar  | unique per provider; stable key for idempotent upserts |
| sku                   | varchar  | provider's own product id, when present |
| gtin_ean              | varchar  | optional barcode, when present |
| product_name          | varchar  | name extracted from HTML |
| provider_brand        | varchar  | brand as scraped |
| provider_size         | varchar  | size as scraped |
| availability          | varchar  | e.g. `in_stock`, `out_of_stock` |
| product_id            | int      | nullable; confirmed foreign key → `products.id` |
| last_seen_at          | datetime | last poll that observed this listing |
| created_at            | datetime | |

Unique: `(provider_id, provider_product_url)`. For matched rows, `product_id` is unique per provider product.

### provider_product_images

Images scraped from a provider product's card. Stored as rows (not an array) so each image can carry display order and primary status.

| Column              | Type     | Notes |
| ------------------- | -------- | ----- |
| id                  | int      | primary key |
| provider_product_id | int      | foreign key → `provider_products.id` |
| url                 | varchar  | image URL |
| position            | int      | display order |
| is_primary          | boolean  | the thumbnail/main image |
| created_at          | datetime | |
| updated_at          | datetime | |

Unique: `(provider_product_id, url)` guards against duplicate image rows when a site reuses or reorders the same URL. The store upserts by url (position is just display order). At most one `is_primary = true` row per provider product.

### provider_product_matches

Fuzzy-match results between a provider product and canonical products. Stores the top candidates and their status so recommendations are separate from human-confirmed decisions.

| Column              | Type     | Notes |
| ------------------- | -------- | ----- |
| id                  | int      | primary key |
| provider_product_id | int      | foreign key → `provider_products.id` |
| product_id          | int      | foreign key → `products.id` |
| score               | decimal  | fuzzy match confidence (0–1) |
| status              | varchar  | `pending`, `confirmed`, `rejected` |
| created_at          | datetime | |
| updated_at          | datetime | when the match status was last changed |

When a match is confirmed, `provider_products.product_id` is set to the same `products.id`.

Unique: `(provider_product_id, product_id)` prevents duplicate candidate rows; at most one `confirmed` row per provider product.

### provider_product_prices

Price observations for a provider product over time.

| Column              | Type      | Notes |
| ------------------- | --------- | ----- |
| id                  | int       | primary key |
| provider_product_id | int       | foreign key → `provider_products.id` |
| scrape_id           | int       | foreign key → `scrapes.id`; which poll observed this |
| price               | decimal   | parsed numeric price |
| currency            | varchar   | e.g. `ARS`, `USD` |
| price_text          | varchar   | raw text from HTML, for auditing |
| created_at          | datetime  | |

Unique: `(provider_product_id, created_at)` guards against duplicate snapshots within one poll.

## Implementation notes

The app persists through the PocketBase Record API only (no SQL, no direct DB file access).

- All `FK` markers above are logical references; PocketBase does not enforce foreign keys. Enforced in application code.
- Unique constraints **are** supported: PocketBase backs collections with SQLite and lets migrations define unique indexes via `$collection->indexes.add(...)`. The `(provider_id, provider_product_url)`, `(provider_product_id, created_at)`, `(provider_product_id, product_id)`, `(provider_product_id, url)`, and `(brand, name, size)` uniques should be declared there, not only in app code.
- `boolean`/`decimal`/`datetime` in this diagram map to PocketBase's `bool`/`number`/`date` collection field types.
- If a future migration moves this model to a relational SQL database (Diesel/SQLx), FK constraints become enforceable schema-level `FOREIGN KEY` clauses.
