// Creates the captures + products collections (mirrors the schema that used to
// live in trailbase/migrations/). No SQL anywhere — schema is defined through
// the PocketBase JS migration API only.
//
// Rules:
//   listRule/viewRule = ""   -> world-readable (the companion app reads prices)
//   createRule/... = null    -> only superuser tokens can write (the scraper)
migrate((app) => {
  const captures = new Collection({
    type: "base",
    name: "captures",
    listRule: "",
    viewRule: "",
    createRule: null,
    updateRule: null,
    deleteRule: null,
    fields: [
      { name: "url", type: "text", required: true },
      { name: "host", type: "text", required: true },
      { name: "captured_at", type: "number", required: true },
      { name: "container_classes", type: "text", required: true },
      { name: "container_id", type: "text" },
      { name: "child_count", type: "number", required: true },
      { name: "detected_cards", type: "number", required: true },
      { name: "created", type: "autodate", onCreate: true, onUpdate: false },
      { name: "updated", type: "autodate", onCreate: true, onUpdate: true },
    ],
  });
  app.save(captures);

  const products = new Collection({
    type: "base",
    name: "products",
    listRule: "",
    viewRule: "",
    createRule: null,
    updateRule: null,
    deleteRule: null,
    fields: [
      {
        name: "capture",
        type: "relation",
        collectionId: captures.id,
        maxSelect: 1,
        cascadeDelete: true,
        required: true,
      },
      { name: "name", type: "text", required: true },
      { name: "price_text", type: "text", required: true },
      { name: "price", type: "number", required: true },
      { name: "created", type: "autodate", onCreate: true, onUpdate: false },
      { name: "updated", type: "autodate", onCreate: true, onUpdate: true },
    ],
  });
  app.save(products);
}, (app) => {
  const products = app.findCollectionByNameOrId("products");
  if (products) {
    app.delete(products);
  }
  const captures = app.findCollectionByNameOrId("captures");
  if (captures) {
    app.delete(captures);
  }
});
