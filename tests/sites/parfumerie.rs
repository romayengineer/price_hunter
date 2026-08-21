use crate::common;

use price_hunter::detect::Product;

fn products() -> Vec<Product> {
    vec![
        Product {
            name: String::from("RABANNE FAME COUTURE EDP 80ML EDICIÓN LIMITADA"),
            price_text: String::new(),
            price: 264000.0,
            ..Default::default()
        },
        Product {
            name: String::from("CAROLINA HERRERA 212 SEXY MEN EDT 100ML"),
            price_text: String::new(),
            price: 165000.0,
            ..Default::default()
        },
        Product {
            name: String::from("BANDERAS SEDUCTION X MEN EDP 100ML"),
            price_text: String::new(),
            price: 54900.0,
            ..Default::default()
        },
        Product {
            name: String::from("CAROLINA HERRERA LA BOMBA EDP 50ML"),
            price_text: String::new(),
            price: 265075.0,
            ..Default::default()
        },
        Product {
            name: String::from(
                "RABANNE INVICTUS VICTORY ELIXIR MEN PARFUM INTENSE 100ML & DESODORANTE & TRAVEL SIZE SET",
            ),
            price_text: String::new(),
            price: 282000.0,
            ..Default::default()
        },
        Product {
            name: String::from("RALPH LAUREN POLO RED EDT 125ML RECARGABLE"),
            price_text: String::new(),
            price: 271200.0,
            ..Default::default()
        },
        Product {
            name: String::from("BENSIMON BOLD PRIVE PARFUM 100ML"),
            price_text: String::new(),
            price: 44993.0,
            ..Default::default()
        },
        Product {
            name: String::from("BURBERRY GODDESS EDP INTENSE 100ML"),
            price_text: String::new(),
            price: 250875.0,
            ..Default::default()
        },
        Product {
            name: String::from("LANCOME IDOLE EDP 100ML RECARGA"),
            price_text: String::new(),
            price: 199200.0,
            ..Default::default()
        },
        Product {
            name: String::from(
                "RABANNE ONE MILLION NIGHT ELIXIR MEN PARFUM ELIXIR 100ML EDICION LIMITADA",
            ),
            price_text: String::new(),
            price: 256000.0,
            ..Default::default()
        },
        Product {
            name: String::from("CAROLINA HERRERA 212 VIP ROSE ELIXIR EDP 80ML"),
            price_text: String::new(),
            price: 339250.0,
            ..Default::default()
        },
        Product {
            name: String::from("RABANNE INVICTUS EDT 200ML"),
            price_text: String::new(),
            price: 266901.0,
            ..Default::default()
        },
        Product {
            name: String::from("DIOR HYPNOTIC POISON EDT 100ML"),
            price_text: String::new(),
            price: 201370.0,
            ..Default::default()
        },
        Product {
            name: String::from("RABANNE FAME EDP 80ML RECARGABLE"),
            price_text: String::new(),
            price: 270000.0,
            ..Default::default()
        },
        Product {
            name: String::from("RABANNE INVICTUS ELIXIR PARFUM INTENSE 100ML"),
            price_text: String::new(),
            price: 285000.0,
            ..Default::default()
        },
        Product {
            name: String::from("SARKANY WHY NOT DESIRE EDP 100ML"),
            price_text: String::new(),
            price: 35993.0,
            ..Default::default()
        },
        Product {
            name: String::from("CHER DIECISIETE EDP 100ML"),
            price_text: String::new(),
            price: 39192.0,
            ..Default::default()
        },
        Product {
            name: String::from(
                "CAROLINA HERRERA 212 MEN EDT 100ML & DESODORANTE 75ML & TRAVEL SIZE 10ML SET",
            ),
            price_text: String::new(),
            price: 259900.0,
            ..Default::default()
        },
        Product {
            name: String::from("SARKANY DRAGONESS EDP 50ML"),
            price_text: String::new(),
            price: 37990.0,
            ..Default::default()
        },
        Product {
            name: String::from(
                "RABANNE INVICTUS MEN EDT 100ML & DESODORANTE 150ML & TRAVEL SIZE 10ML SET",
            ),
            price_text: String::new(),
            price: 235000.0,
            ..Default::default()
        },
        Product {
            name: String::from("RABANNE INVICTUS VICTORY ELIXIR PARFUM INTENSE 100ML"),
            price_text: String::new(),
            price: 279400.0,
            ..Default::default()
        },
        Product {
            name: String::from("BANDERAS POWER OF SEDUCTION EDT 200ML"),
            price_text: String::new(),
            price: 58203.0,
            ..Default::default()
        },
        Product {
            name: String::from("CAROLINA HERRERA 212 VIP ROSÉ EDP 80ML"),
            price_text: String::new(),
            price: 310500.0,
            ..Default::default()
        },
        Product {
            name: String::from("RABANNE BLACK XS MEN EDT 100ML"),
            price_text: String::new(),
            price: 195000.0,
            ..Default::default()
        },
    ]
}

#[test]
fn extracts_all_products_from_parfumerie_fixture() {
    common::assert_fixture("tests/fixtures/parfumerie.html", &products(), "products");
}
