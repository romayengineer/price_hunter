// Makes provider_products.product_id nullable so canonical products can be
// re-imported: unlink provider_products before deleting products.
// The merged init migration already declares product_id without required:true,
// but pre-merge pb_data instances still have NOT NULL — this migration
// alters the live collection.
migrate(
  (app) => {
    const col = app.findCollectionByNameOrId("provider_products");
    const field = col.fields.getByName("product_id");
    if (field) {
      field.required = false;
      // Some PocketBase builds store required in options; clear both.
      if (field.options) {
        field.options.required = false;
      }
      app.save(col);
    }
  },
  (app) => {
    const col = app.findCollectionByNameOrId("provider_products");
    const field = col.fields.getByName("product_id");
    if (field) {
      field.required = true;
      if (field.options) {
        field.options.required = true;
      }
      app.save(col);
    }
  },
);
