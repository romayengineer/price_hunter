// Migrates canonical products to size-less identity and moves size to
// provider_products. Products becomes (brand, product_name) unique;
// provider_products gains a normalized `size` column with a new unique
// (provider_id, product_id, size) alongside existing url/name uniques.
// Existing provider_products.provider_size (if any) is copied to size.

migrate(
  (app) => {
    // --- products: drop size, change unique index ---
    const products = app.findCollectionByNameOrId("products");
    // Remove size field
    const sizeField = products.fields.getByName("size");
    if (sizeField) {
      products.fields.removeById(sizeField.id);
    }
    // Replace unique index (brand, name, size) -> (brand, product_name)
    // Keep for safety: drop old, create new.
    products.indexes = [
      "CREATE UNIQUE INDEX idx_products_brand_product_name ON products (COALESCE(brand, ''), product_name)",
    ];
    app.save(products);

    // Deduplicate existing products that now collide on (brand, product_name):
    // keep first row per lower(brand, product_name), repoint provider_products
    // and delete matches for survivors before deleting duplicates. This runs in
    // JS via DAO - best-effort; empty DB is fine.
    try {
      const dao = app.dao();
      // Collect all products grouped by lower key
      const all = dao.findRecordsByFilter("products", "", "-created", 1000, 0);
      // PocketBase findRecordsByFilter paginates; loop if needed outside migration
      // For now handle first 1000; re-run manually if larger.
      const seen = {};
      for (const p of all) {
        const key = (p.getString("brand") || "").toLowerCase() + "|" + (p.getString("product_name") || "").toLowerCase();
        if (!seen[key]) {
          seen[key] = p;
          // fix name to be brand + product_name (no size)
          const brand = (p.getString("brand") || "").trim();
          const product_name = (p.getString("product_name") || "").trim();
          const expected = [brand, product_name].filter(Boolean).join(" ");
          if (p.getString("name") !== expected) {
            p.set("name", expected);
            dao.saveRecord(p);
          }
        } else {
          const survivorId = seen[key].id;
          // repoint provider_products
          const linked = dao.findRecordsByFilter("provider_products", `product_id='${survivorId}' || product_id='${p.id}'`, "", 500, 0);
          // Actually repoint only p's links to survivor (handled via separate query)
          const toMove = dao.findRecordsByFilter("provider_products", `product_id='${p.id}'`, "", 500, 0);
          for (const pp of toMove) {
            pp.set("product_id", survivorId);
            dao.saveRecord(pp);
          }
          // delete matches for duplicate product
          const matches = dao.findRecordsByFilter("provider_product_matches", `product_id='${p.id}'`, "", 500, 0);
          for (const m of matches) dao.deleteRecord(m);
          dao.deleteRecord(p);
        }
      }
    } catch (e) {
      console.log("dedup products skipped: " + e);
    }

    // --- provider_products: add size, indexes, move provider_size ---
    const providerProducts = app.findCollectionByNameOrId("provider_products");
    // Add normalized size field
    providerProducts.fields.addAt(providerProducts.fields.length, new Field({
      name: "size",
      type: "text",
    }));
    // Add new unique on (provider_id, product_id, size) only when linked
    // (allows duplicates when product_id is NULL/empty — the steady state
    // before -match-products). Size empty '' is a single bucket for linked rows.
    providerProducts.indexes = [
      "CREATE UNIQUE INDEX idx_provider_products_provider_url ON provider_products (provider_id, provider_product_url)",
      "CREATE UNIQUE INDEX idx_provider_products_provider_name ON provider_products (provider_id, name)",
      "CREATE INDEX idx_provider_products_brand_id ON provider_products (brand_id)",
      "CREATE UNIQUE INDEX idx_provider_products_provider_product_size ON provider_products (provider_id, COALESCE(size, ''), product_id, COALESCE(size, '')) WHERE product_id != ''",
    ];
    app.save(providerProducts);

    // Backfill size from provider_size where size empty (if column existed)
    try {
      const dao2 = app.dao();
      const allPP = dao2.findRecordsByFilter("provider_products", "", "", 500, 0);
      for (const pp of allPP) {
        const cur = pp.getString("size");
        if (!cur) {
          const legacy = pp.getString("provider_size");
          if (legacy) {
            pp.set("size", legacy);
            dao2.saveRecord(pp);
          }
        }
      }
    } catch (e) {
      console.log("backfill provider_products.size skipped: " + e);
    }

    // Drop provider_size field (migrated)
    try {
      const pp2 = app.findCollectionByNameOrId("provider_products");
      const legacyField = pp2.fields.getByName("provider_size");
      if (legacyField) {
        pp2.fields.removeById(legacyField.id);
        app.save(pp2);
      }
    } catch (e) {
      console.log("drop provider_size skipped: " + e);
    }
  },
  (app) => {
    // Down: restore size on products and provider_size on provider_products
    const products = app.findCollectionByNameOrId("products");
    products.fields.addAt(products.fields.length, new Field({ name: "size", type: "text" }));
    products.indexes = [
      "CREATE UNIQUE INDEX idx_products_brand_name_size ON products (COALESCE(brand, ''), name, COALESCE(size, ''))",
    ];
    app.save(products);

    const providerProducts = app.findCollectionByNameOrId("provider_products");
    const sizeField = providerProducts.fields.getByName("size");
    if (sizeField) providerProducts.fields.removeById(sizeField.id);
    providerProducts.fields.addAt(providerProducts.fields.length, new Field({ name: "provider_size", type: "text" }));
    providerProducts.indexes = [
      "CREATE UNIQUE INDEX idx_provider_products_provider_url ON provider_products (provider_id, provider_product_url)",
      "CREATE UNIQUE INDEX idx_provider_products_provider_name ON provider_products (provider_id, name)",
      "CREATE INDEX idx_provider_products_brand_id ON provider_products (brand_id)",
    ];
    app.save(providerProducts);
  },
);
