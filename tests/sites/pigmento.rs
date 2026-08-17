use crate::common;

use price_hunter::detect::Product;

fn products() -> Vec<Product> {
    vec![
        Product {
            name: String::from("Gold Fresh Couture EDP 100 Ml"),
            price_text: String::new(),
            price: 95400.0,
            ..Default::default()
        },
        Product {
            name: String::from("Funny EDT 100 Ml"),
            price_text: String::new(),
            price: 89000.0,
            ..Default::default()
        },
        Product {
            name: String::from("Fresh Couture EDT 100 Ml"),
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
            name: String::from("Pink Fresh Couture EDT 100 Ml"),
            price_text: String::new(),
            price: 89000.0,
            ..Default::default()
        },
        Product {
            name: String::from("Red Jeans EDT 75 Ml"),
            price_text: String::new(),
            price: 79990.0,
            ..Default::default()
        },
        Product {
            name: String::from("Defy Men EDP 200 Ml"),
            price_text: String::new(),
            price: 187500.0,
            ..Default::default()
        },
        Product {
            name: String::from("Orissima EDP 30 Ml"),
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
            name: String::from("Rumba Fever EDT 100 Ml"),
            price_text: String::new(),
            price: 90900.0,
            ..Default::default()
        },
        Product {
            name: String::from("Green Tea Lavender EDT 100 Ml"),
            price_text: String::new(),
            price: 52030.0,
            ..Default::default()
        },
        Product {
            name: String::from("Blue Night EDP 200 Ml"),
            price_text: String::new(),
            price: 48594.0,
            ..Default::default()
        },
        Product {
            name: String::from("212 Nyc EDT 30 Ml"),
            price_text: String::new(),
            price: 102000.0,
            ..Default::default()
        },
        Product {
            name: String::from("One Million EDT 200 Ml"),
            price_text: String::new(),
            price: 266901.0,
            ..Default::default()
        },
        Product {
            name: String::from("212 Vip Black Men EDP 200 Ml"),
            price_text: String::new(),
            price: 264063.0,
            ..Default::default()
        },
        Product {
            name: String::from("212 VIP Men EDT Edición Limitada 200 Ml"),
            price_text: String::new(),
            price: 256368.0,
            ..Default::default()
        },
        Product {
            name: String::from("Man Rock On EDT 125 Ml"),
            price_text: String::new(),
            price: 102900.0,
            ..Default::default()
        },
        Product {
            name: String::from("Kiss Sexy EDT 100 Ml"),
            price_text: String::new(),
            price: 105900.0,
            ..Default::default()
        },
        Product {
            name: String::from("L'eau D'issey Intense Pour Homme EDT 125 Ml"),
            price_text: String::new(),
            price: 160800.0,
            ..Default::default()
        },
        Product {
            name: String::from("Lune EDP 100 Ml"),
            price_text: String::new(),
            price: 43500.0,
            ..Default::default()
        },
        Product {
            name: String::from("Phantom EDT Refillable 150 Ml"),
            price_text: String::new(),
            price: 264342.0,
            ..Default::default()
        },
        Product {
            name: String::from("Drakkar Intense EDP 100 Ml"),
            price_text: String::new(),
            price: 50100.0,
            ..Default::default()
        },
        Product {
            name: String::from("Original EDT 100 Ml"),
            price_text: String::new(),
            price: 268317.0,
            ..Default::default()
        },
        Product {
            name: String::from("La Vie Est Belle Intensement EDP 100 Ml"),
            price_text: String::new(),
            price: 239000.0,
            ..Default::default()
        },
        Product {
            name: String::from("Cool Water Men EDT 75 Ml"),
            price_text: String::new(),
            price: 137214.0,
            ..Default::default()
        },
        Product {
            name: String::from("Incanto Di Fiore EDT 100 Ml"),
            price_text: String::new(),
            price: 25993.0,
            ..Default::default()
        },
        Product {
            name: String::from("Incanto Dolce Pistaccio EDT 100 Ml"),
            price_text: String::new(),
            price: 25993.0,
            ..Default::default()
        },
        Product {
            name: String::from("Incanto Eterno EDT 100 Ml"),
            price_text: String::new(),
            price: 25993.0,
            ..Default::default()
        },
        Product {
            name: String::from("Perfume Why Not Excess EDP 150ml"),
            price_text: String::new(),
            price: 48093.0,
            ..Default::default()
        },
        Product {
            name: String::from("Voyage EDT 100 Ml"),
            price_text: String::new(),
            price: 38624.0,
            ..Default::default()
        },
        Product {
            name: String::from("L'Eau D'Issey Pour Homme Sport EDT 100 Ml"),
            price_text: String::new(),
            price: 156000.0,
            ..Default::default()
        },
        Product {
            name: String::from("Set Tommy Men EDT 100 Ml + After Shave"),
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
