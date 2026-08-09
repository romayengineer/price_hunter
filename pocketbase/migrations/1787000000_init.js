// Creates the price_hunter schema (mirrors DATABASE.md):
//
//   providers, products (canonical), scrapes, provider_products,
//   provider_product_images, provider_product_matches, provider_product_prices
//
// No SQL anywhere — schema is defined through the PocketBase JS migration API
// only.
//
// Rules:
//   listRule/viewRule = ""   -> world-readable (the companion app reads prices)
//   createRule/... = null    -> only superuser tokens can write (the scraper)
//
// Note: `app.save()` in this PocketBase build returns nothing (Go's
// `Save(model) error`), so the collection id is read from the in-place-mutated
// object after saving — never from the return value.
migrate(
  (app) => {
    const providers = new Collection({
      type: "base",
      name: "providers",
      listRule: "",
      viewRule: "",
      createRule: null,
      updateRule: null,
      deleteRule: null,
      indexes: ["CREATE UNIQUE INDEX idx_providers_domain ON providers (domain)"],
      fields: [
        { name: "domain", type: "text", required: true },
        { name: "name", type: "text", required: true },
        { name: "enabled", type: "bool", required: true },
        { name: "default_currency", type: "text" },
        { name: "created", type: "autodate", onCreate: true, onUpdate: false },
        { name: "updated", type: "autodate", onCreate: true, onUpdate: true },
      ],
    });
    app.save(providers);

    const products = new Collection({
      type: "base",
      name: "products",
      listRule: "",
      viewRule: "",
      createRule: null,
      updateRule: null,
      deleteRule: null,
      indexes: [
        "CREATE UNIQUE INDEX idx_products_brand_name_size ON products (COALESCE(brand, ''), name, COALESCE(size, ''))",
      ],
      fields: [
        { name: "brand", type: "text" },
        { name: "name", type: "text", required: true },
        { name: "size", type: "text" },
        { name: "category", type: "text" },
        { name: "active", type: "bool", required: true },
        { name: "created", type: "autodate", onCreate: true, onUpdate: false },
        { name: "updated", type: "autodate", onCreate: true, onUpdate: true },
      ],
    });
    app.save(products);

    const scrapes = new Collection({
      type: "base",
      name: "scrapes",
      listRule: "",
      viewRule: "",
      createRule: null,
      updateRule: null,
      deleteRule: null,
      fields: [
        {
          name: "provider_id",
          type: "relation",
          collectionId: providers.id,
          maxSelect: 1,
          required: true,
        },
        { name: "url", type: "text", required: true },
        { name: "scraped_at", type: "date", required: true },
        { name: "status", type: "text", required: true },
        { name: "capture_path", type: "text" },
        { name: "product_count", type: "number", required: true },
        { name: "container_class", type: "text" },
        { name: "created", type: "autodate", onCreate: true, onUpdate: false },
        { name: "updated", type: "autodate", onCreate: true, onUpdate: true },
      ],
    });
    app.save(scrapes);

    const providerProducts = new Collection({
      type: "base",
      name: "provider_products",
      listRule: "",
      viewRule: "",
      createRule: null,
      updateRule: null,
      deleteRule: null,
      indexes: [
        "CREATE UNIQUE INDEX idx_provider_products_provider_url ON provider_products (provider_id, provider_product_url)",
        "CREATE UNIQUE INDEX idx_provider_products_provider_name ON provider_products (provider_id, product_name)",
      ],
      fields: [
        {
          name: "provider_id",
          type: "relation",
          collectionId: providers.id,
          maxSelect: 1,
          required: true,
        },
        { name: "provider_product_url", type: "text", required: true },
        { name: "sku", type: "text" },
        { name: "gtin_ean", type: "text" },
        { name: "product_name", type: "text", required: true },
        { name: "provider_brand", type: "text" },
        { name: "provider_size", type: "text" },
        { name: "availability", type: "text" },
        {
          name: "product_id",
          type: "relation",
          collectionId: products.id,
          maxSelect: 1,
        },
        { name: "last_seen_at", type: "date", required: true },
        { name: "created", type: "autodate", onCreate: true, onUpdate: false },
        { name: "updated", type: "autodate", onCreate: true, onUpdate: true },
      ],
    });
    app.save(providerProducts);

    const providerProductImages = new Collection({
      type: "base",
      name: "provider_product_images",
      listRule: "",
      viewRule: "",
      createRule: null,
      updateRule: null,
      deleteRule: null,
      indexes: [
        "CREATE UNIQUE INDEX idx_provider_product_images_url ON provider_product_images (provider_product_id, url)",
      ],
      fields: [
        {
          name: "provider_product_id",
          type: "relation",
          collectionId: providerProducts.id,
          maxSelect: 1,
          required: true,
        },
        { name: "url", type: "text", required: true },
        // position/is_primary are NOT required: PocketBase treats the zero
        // value (position 0, is_primary false) as blank, and both are written
        // on every image sync.
        { name: "position", type: "number" },
        { name: "is_primary", type: "bool" },
        { name: "created", type: "autodate", onCreate: true, onUpdate: false },
        { name: "updated", type: "autodate", onCreate: true, onUpdate: true },
      ],
    });
    app.save(providerProductImages);

    const productMatches = new Collection({
      type: "base",
      name: "provider_product_matches",
      listRule: "",
      viewRule: "",
      createRule: null,
      updateRule: null,
      deleteRule: null,
      indexes: [
        "CREATE UNIQUE INDEX idx_provider_product_matches_pair ON provider_product_matches (provider_product_id, product_id)",
      ],
      fields: [
        {
          name: "provider_product_id",
          type: "relation",
          collectionId: providerProducts.id,
          maxSelect: 1,
          required: true,
        },
        {
          name: "product_id",
          type: "relation",
          collectionId: products.id,
          maxSelect: 1,
          required: true,
        },
        { name: "score", type: "number", required: true },
        { name: "status", type: "text", required: true },
        { name: "created", type: "autodate", onCreate: true, onUpdate: false },
        { name: "updated", type: "autodate", onCreate: true, onUpdate: true },
      ],
    });
    app.save(productMatches);

    const providerProductPrices = new Collection({
      type: "base",
      name: "provider_product_prices",
      listRule: "",
      viewRule: "",
      createRule: null,
      updateRule: null,
      deleteRule: null,
      indexes: [
        "CREATE UNIQUE INDEX idx_provider_product_prices_scrape ON provider_product_prices (provider_product_id, scrape_id)",
      ],
      fields: [
        {
          name: "provider_product_id",
          type: "relation",
          collectionId: providerProducts.id,
          maxSelect: 1,
          required: true,
        },
        {
          name: "scrape_id",
          type: "relation",
          collectionId: scrapes.id,
          maxSelect: 1,
          required: true,
        },
        { name: "price", type: "number", required: true },
        { name: "currency", type: "text" },
        { name: "price_text", type: "text", required: true },
        { name: "created", type: "autodate", onCreate: true, onUpdate: false },
        { name: "updated", type: "autodate", onCreate: true, onUpdate: true },
      ],
    });
    app.save(providerProductPrices);
  },
  (app) => {
    for (const name of [
      "products",
      "provider_product_images",
      "provider_product_matches",
      "provider_product_prices",
      "provider_products",
      "providers",
      "scrapes",
    ]) {
      const collection = app.findCollectionByNameOrId(name);
      if (collection) {
        app.delete(collection);
      }
    }
  },
);
