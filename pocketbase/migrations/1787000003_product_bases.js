// Creates the product_bases collection for the base/variant split (ADR-0001).
// product_bases holds abstract catalog entries (brand, product_name) without size
// — never matched or priced directly. products becomes the variant table:
// one row per (base_id, size) with mandatory size.
// Fresh start: existing products rows are cleared via app code, not here.

migrate(
  (app) => {
    const productBases = new Collection({
      type: "base",
      name: "product_bases",
      listRule: "",
      viewRule: "",
      createRule: null,
      updateRule: null,
      deleteRule: null,
      indexes: ["CREATE UNIQUE INDEX idx_product_bases_brand_name ON product_bases (brand, product_name)"],
      fields: [
        { name: "brand", type: "text", required: true },
        { name: "product_name", type: "text", required: true },
        { name: "category", type: "text" },
        { name: "active", type: "bool", required: true },
        { name: "created", type: "autodate", onCreate: true, onUpdate: false },
        { name: "updated", type: "autodate", onCreate: true, onUpdate: true },
      ],
    });
    app.save(productBases);
    // refetch to ensure id is populated
    const bases = app.findCollectionByNameOrId("product_bases");
    const basesId = bases ? bases.id : productBases.id;
    if (!basesId) {
      throw new Error("product_bases id is blank after save; productBases=" + JSON.stringify(productBases) + " bases=" + JSON.stringify(bases));
    }

    // Add optional base relation to existing products (variants). Keep brand/
    // product_name columns for backward compat during rollout — later migration
    // will drop them once code no longer writes them.
    const products = app.findCollectionByNameOrId("products");
    const relField = new Field({
      system: false,
      hidden: false,
      id: "prod_base_id",
      name: "product_base_id",
      type: "relation",
      required: false,
      presentable: false,
      unique: false,
      help: "",
      collectionId: basesId,
      cascadeDelete: false,
      minSelect: null,
      maxSelect: 1,
      displayFields: null,
    });
    products.fields.addAt(products.fields.length, relField);
    // Variant uniqueness is (base_id, size) — keep old idx_products_brand_name_size for
    // existing rows, add new index for variant rows.
    products.indexes.push(
      "CREATE UNIQUE INDEX idx_products_base_size ON products (product_base_id, COALESCE(size, ''))",
    );
    app.save(products);
  },
  (app) => {
    const products = app.findCollectionByNameOrId("products");
    if (products) {
      const f = products.fields.getById("prod_base_id");
      if (f) {
        products.fields.removeById("prod_base_id");
      }
      products.indexes = products.indexes.filter(
        (idx) => !idx.includes("idx_products_base_size"),
      );
      app.save(products);
    }
    const col = app.findCollectionByNameOrId("product_bases");
    if (col) {
      app.delete(col);
    }
  },
);
