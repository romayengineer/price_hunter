// Adds a nullable `brand_id` relation to `provider_products` pointing at the
// `brand` collection. `-match-brands` fills it in from a linked product's
// brand or a fuzzy brand match; `brand_id = null` marks provider products
// whose brand is not yet known (used to extend the brand table).
migrate(
  (app) => {
    const pp = app.findCollectionByNameOrId("provider_products");
    const brand = app.findCollectionByNameOrId("brand");
    if (!pp.fields.find((f) => f.name === "brand_id")) {
      pp.fields.add(
        new RelationField({
          name: "brand_id",
          collectionId: brand.id,
          maxSelect: 1,
          required: false,
        }),
      );
    }
    if (!pp.indexes.some((i) => i.includes("idx_provider_products_brand_id"))) {
      pp.indexes.push(
        "CREATE INDEX idx_provider_products_brand_id ON provider_products (brand_id)",
      );
    }
    app.save(pp);
  },
  (app) => {
    const pp = app.findCollectionByNameOrId("provider_products");
    if (pp) {
      const field = pp.fields.find((f) => f.name === "brand_id");
      if (field) {
        pp.fields.removeById(field.id);
        pp.indexes = pp.indexes.filter(
          (i) => !i.includes("idx_provider_products_brand_id"),
        );
        app.save(pp);
      }
    }
  },
);
