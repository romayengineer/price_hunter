// Renames `provider_products.product_name` to `name` (mirrors DATABASE.md).
//
// The field is renamed in place (same field id), so PocketBase runs
// `ALTER TABLE ... RENAME COLUMN` and keeps existing data. The unique index
// SQL is updated to reference the new column name.
migrate(
  (app) => {
    const collection = app.findCollectionByNameOrId("provider_products");
    const field = collection.fields.getByName("product_name");
    if (field) {
      field.name = "name";
    }
    collection.indexes = collection.indexes.map((sql) =>
      sql.replace("(provider_id, product_name)", "(provider_id, name)")
    );
    app.save(collection);
  },
  (app) => {
    const collection = app.findCollectionByNameOrId("provider_products");
    const field = collection.fields.getByName("name");
    if (field) {
      field.name = "product_name";
    }
    collection.indexes = collection.indexes.map((sql) =>
      sql.replace("(provider_id, name)", "(provider_id, product_name)")
    );
    app.save(collection);
  },
);
