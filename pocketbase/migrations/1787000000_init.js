// Creates the price_hunter schema (mirrors DATABASE.md):
//
//   providers, products (canonical), scrapes, provider_products,
//   product_images, product_matches, provider_prices
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

    const productImages = new Collection({
      type: "base",
      name: "product_images",
      listRule: "",
      viewRule: "",
      createRule: null,
      updateRule: null,
      deleteRule: null,
      indexes: [
        "CREATE UNIQUE INDEX idx_product_images_position ON product_images (provider_product_id, position)",
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
        { name: "position", type: "number", required: true },
        { name: "is_primary", type: "bool", required: true },
        { name: "created", type: "autodate", onCreate: true, onUpdate: false },
        { name: "updated", type: "autodate", onCreate: true, onUpdate: true },
      ],
    });
    app.save(productImages);

    const productMatches = new Collection({
      type: "base",
      name: "product_matches",
      listRule: "",
      viewRule: "",
      createRule: null,
      updateRule: null,
      deleteRule: null,
      indexes: [
        "CREATE UNIQUE INDEX idx_product_matches_pair ON product_matches (provider_product_id, product_id)",
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

    const providerPrices = new Collection({
      type: "base",
      name: "provider_prices",
      listRule: "",
      viewRule: "",
      createRule: null,
      updateRule: null,
      deleteRule: null,
      indexes: [
        "CREATE UNIQUE INDEX idx_provider_prices_scrape ON provider_prices (provider_product_id, scrape_id)",
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
    app.save(providerPrices);
  },
  (app) => {
    for (const name of [
      "provider_prices",
      "product_matches",
      "product_images",
      "provider_products",
      "scrapes",
      "products",
      "providers",
    ]) {
      const collection = app.findCollectionByNameOrId(name);
      if (collection) {
        app.delete(collection);
      }
    }
  },
);
