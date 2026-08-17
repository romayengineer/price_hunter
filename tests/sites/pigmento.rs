use crate::common;

use price_hunter::detect::Product;

fn products() -> Vec<Product> {
    vec![
        Product {
            name: String::from("Moschino Gold Fresh Couture EDP 100 Ml"),
            price_text: String::new(),
            price: 95400.0,
            ..Default::default()
        },
        Product {
            name: String::from("Moschino Funny EDT 100 Ml"),
            price_text: String::new(),
            price: 89000.0,
            ..Default::default()
        },
        Product {
            name: String::from("Moschino Fresh Couture EDT 100 Ml"),
            price_text: String::new(),
            price: 94900.0,
            ..Default::default()
        },
        Product {
            name: String::from("Adidas Vibes Smooth Pace EDP Unisex 100 Ml"),
            price_text: String::new(),
            price: 21450.0,
            ..Default::default()
        },
        Product {
            name: String::from("Moschino Pink Fresh Couture EDT 100 Ml"),
            price_text: String::new(),
            price: 89000.0,
            ..Default::default()
        },
        Product {
            name: String::from("Versace Red Jeans EDT 75 Ml"),
            price_text: String::new(),
            price: 79990.0,
            ..Default::default()
        },
        Product {
            name: String::from("Calvin Klein Defy Men EDP 200 Ml"),
            price_text: String::new(),
            price: 187500.0,
            ..Default::default()
        },
        Product {
            name: String::from("Ted Lapidus Orissima EDP 30 Ml"),
            price_text: String::new(),
            price: 56490.0,
            ..Default::default()
        },
        Product {
            name: String::from("Azzaro Sport EDT 100 Ml"),
            price_text: String::new(),
            price: 111000.0,
            ..Default::default()
        },
        Product {
            name: String::from("Ted Lapidus Rumba Fever EDT 100 Ml"),
            price_text: String::new(),
            price: 90900.0,
            ..Default::default()
        },
        Product {
            name: String::from("Elizabeth Arden Green Tea Lavender EDT 100 Ml"),
            price_text: String::new(),
            price: 52030.0,
            ..Default::default()
        },
        Product {
            name: String::from("Bensimon Blue Night EDP 200 Ml"),
            price_text: String::new(),
            price: 48594.0,
            ..Default::default()
        },
        Product {
            name: String::from("Carolina Herrera 212 Nyc EDT 30 Ml"),
            price_text: String::new(),
            price: 102000.0,
            ..Default::default()
        },
        Product {
            name: String::from("Rabanne One Million EDT 200 Ml"),
            price_text: String::new(),
            price: 266901.0,
            ..Default::default()
        },
        Product {
            name: String::from("Carolina Herrera 212 Vip Black Men EDP 200 Ml"),
            price_text: String::new(),
            price: 264063.0,
            ..Default::default()
        },
        Product {
            name: String::from("Carolina Herrera 212 VIP Men EDT Edición Limitada 200 Ml"),
            price_text: String::new(),
            price: 256368.0,
            ..Default::default()
        },
        Product {
            name: String::from("Halloween Man Rock On EDT 125 Ml"),
            price_text: String::new(),
            price: 102900.0,
            ..Default::default()
        },
        Product {
            name: String::from("Halloween Kiss Sexy EDT 100 Ml"),
            price_text: String::new(),
            price: 105900.0,
            ..Default::default()
        },
        Product {
            name: String::from("Issey Miyake L'eau D'issey Intense Pour Homme EDT 125 Ml"),
            price_text: String::new(),
            price: 160800.0,
            ..Default::default()
        },
        Product {
            name: String::from("Uma Lune EDP 100 Ml"),
            price_text: String::new(),
            price: 43500.0,
            ..Default::default()
        },
        Product {
            name: String::from("Rabanne Phantom EDT Refillable 150 Ml"),
            price_text: String::new(),
            price: 264342.0,
            ..Default::default()
        },
        Product {
            name: String::from("Guy Laroche Drakkar Intense EDP 100 Ml"),
            price_text: String::new(),
            price: 50100.0,
            ..Default::default()
        },
        Product {
            name: String::from("Dolce & Gabbana Original EDT 100 Ml"),
            price_text: String::new(),
            price: 268317.0,
            ..Default::default()
        },
        Product {
            name: String::from("Lancome La Vie Est Belle Intensement EDP 100 Ml"),
            price_text: String::new(),
            price: 239000.0,
            ..Default::default()
        },
        Product {
            name: String::from("Davidoff Cool Water Men EDT 75 Ml"),
            price_text: String::new(),
            price: 137214.0,
            ..Default::default()
        },
        Product {
            name: String::from("Tucci Incanto Di Fiore EDT 100 Ml"),
            price_text: String::new(),
            price: 25993.0,
            ..Default::default()
        },
        Product {
            name: String::from("Tucci Incanto Dolce Pistaccio EDT 100 Ml"),
            price_text: String::new(),
            price: 25993.0,
            ..Default::default()
        },
        Product {
            name: String::from("Tucci Incanto Eterno EDT 100 Ml"),
            price_text: String::new(),
            price: 25993.0,
            ..Default::default()
        },
        Product {
            name: String::from("Sarkany Perfume Why Not Excess EDP 150ml"),
            price_text: String::new(),
            price: 48093.0,
            ..Default::default()
        },
        Product {
            name: String::from("Nautica Voyage EDT 100 Ml"),
            price_text: String::new(),
            price: 38624.0,
            ..Default::default()
        },
        Product {
            name: String::from("Issey Miyake L'Eau D'Issey Pour Homme Sport EDT 100 Ml"),
            price_text: String::new(),
            price: 156000.0,
            ..Default::default()
        },
        Product {
            name: String::from("Tommy Hilfiger Set Tommy Men EDT 100 Ml + After Shave"),
            price_text: String::new(),
            price: 182000.0,
            ..Default::default()
        },
    ]
}

#[test]
fn extracts_all_prices_from_pigmento_fixture() {
    common::assert_fixture(
        "tests/fixtures/pigmento.html",
        &products(),
        "vtex-search-result-3-x-gallery",
    );
}
