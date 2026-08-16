// Creates the `brand` collection holding the canonical brand list imported
// from brands.csv (single column). It will be used later to flag
// provider_products whose names don't contain any known brand.
migrate(
  (app) => {
    const brands = new Collection({
      type: "base",
      name: "brand",
      listRule: "",
      viewRule: "",
      createRule: null,
      updateRule: null,
      deleteRule: null,
      indexes: ["CREATE UNIQUE INDEX idx_brand_name ON brand (name)"],
      fields: [
        { name: "name", type: "text", required: true },
        { name: "created", type: "autodate", onCreate: true, onUpdate: false },
        { name: "updated", type: "autodate", onCreate: true, onUpdate: true },
      ],
    });
    app.save(brands);
  },
  (app) => {
    const collection = app.findCollectionByNameOrId("brand");
    if (collection) {
      app.delete(collection);
    }
  },
);
