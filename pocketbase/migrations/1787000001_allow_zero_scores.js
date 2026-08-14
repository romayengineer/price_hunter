// Makes `provider_product_matches.score` not required so zero scores can be
// stored. PocketBase treats `required` number fields as blank when the value
// is 0, which rejected the exact-0.0 comparisons written by the matcher.
migrate(
  (app) => {
    const collection = app.findCollectionByNameOrId("provider_product_matches");
    const field = collection.fields.find((f) => f.name === "score");
    if (!field) {
      throw new Error("score field not found in provider_product_matches");
    }
    field.required = false;
    app.save(collection);
  },
  (app) => {
    const collection = app.findCollectionByNameOrId("provider_product_matches");
    const field = collection.fields.find((f) => f.name === "score");
    if (field) {
      field.required = true;
      app.save(collection);
    }
  },
);
