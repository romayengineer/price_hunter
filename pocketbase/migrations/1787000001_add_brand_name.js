// Adds provider_products.brand_name - raw brand text captured during scraping.
// Persisted from Product.brand (separate + enriched into name). Resolved to
// brand_id by -match-brands even when brand is unknown at scrape time.
migrate(
  (app) => {
    const col = app.findCollectionByNameOrId("provider_products");
    col.fields.addAt(col.fields.length - 2, new Field({
      system: false,
      id: "pp_brand_name",
      name: "brand_name",
      type: "text",
      required: false,
      presentable: false,
      unique: false,
      options: { min: null, max: null, pattern: "" },
    }));
    app.save(col);
  },
  (app) => {
    const col = app.findCollectionByNameOrId("provider_products");
    const field = col.fields.getById("pp_brand_name");
    if (field) {
      col.fields.removeById("pp_brand_name");
      app.save(col);
    }
  },
);
