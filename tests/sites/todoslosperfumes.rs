use crate::common;

use price_hunter::detect::Product;

fn products() -> Vec<Product> {
    vec![
        Product {
            name: String::from("light blue homme edp 50 ml"),
            price_text: String::new(),
            price: 242100.0,
            ..Default::default()
        },
        Product {
            name: String::from("light blue homme edp 200 ml"),
            price_text: String::new(),
            price: 398700.0,
            ..Default::default()
        },
        Product {
            name: String::from("light blue femme edp 50 ml"),
            price_text: String::new(),
            price: 246600.0,
            ..Default::default()
        },
        Product {
            name: String::from("light blue femme edp 200 ml"),
            price_text: String::new(),
            price: 403200.0,
            ..Default::default()
        },
        Product {
            name: String::from("paula aura edt 100 ml"),
            price_text: String::new(),
            price: 39060.0,
            ..Default::default()
        },
        Product {
            name: String::from("cabochard edt 100 ml"),
            price_text: String::new(),
            price: 143100.0,
            ..Default::default()
        },
        Product {
            name: String::from("toy 2 yummy edp 100 ml"),
            price_text: String::new(),
            price: 200700.0,
            ..Default::default()
        },
        Product {
            name: String::from("toy 2 gummy edp 100 ml"),
            price_text: String::new(),
            price: 200700.0,
            ..Default::default()
        },
        Product {
            name: String::from("alien pulp edp 30 ml"),
            price_text: String::new(),
            price: 187200.0,
            ..Default::default()
        },
        Product {
            name: String::from("refill alien pulp edp 100 ml"),
            price_text: String::new(),
            price: 248400.0,
            ..Default::default()
        },
        Product {
            name: String::from("crystal emerald edp 90 ml"),
            price_text: String::new(),
            price: 295200.0,
            ..Default::default()
        },
        Product {
            name: String::from("alien pulp edp 90 ml"),
            price_text: String::new(),
            price: 310500.0,
            ..Default::default()
        },
        Product {
            name: String::from("my devotion edpi 100 ml"),
            price_text: String::new(),
            price: 325800.0,
            ..Default::default()
        },
        Product {
            name: String::from("the secret prive edp 100 ml"),
            price_text: String::new(),
            price: 61110.0,
            ..Default::default()
        },
        Product {
            name: String::from("her secret prive edp 80 ml"),
            price_text: String::new(),
            price: 52020.0,
            ..Default::default()
        },
        Product {
            name: String::from("scandal elixir parfum 80 ml"),
            price_text: String::new(),
            price: 287100.0,
            ..Default::default()
        },
        Product {
            name: String::from("scandal elixir parfum 100 ml"),
            price_text: String::new(),
            price: 268785.0,
            ..Default::default()
        },
        Product {
            name: String::from("nina pistaccio edp 30 ml"),
            price_text: String::new(),
            price: 137025.0,
            ..Default::default()
        },
        Product {
            name: String::from("nina pistaccio edp 50 ml"),
            price_text: String::new(),
            price: 174825.0,
            ..Default::default()
        },
        Product {
            name: String::from("nina pistaccio edp 80 ml"),
            price_text: String::new(),
            price: 216405.0,
            ..Default::default()
        },
        Product {
            name: String::from("dalia absoluta elixir parfum 100 ml"),
            price_text: String::new(),
            price: 77661.0,
            ..Default::default()
        },
        Product {
            name: String::from("eau d'orange verte edc 200 ml"),
            price_text: String::new(),
            price: 243900.0,
            ..Default::default()
        },
        Product {
            name: String::from("bottled beyond edt intense 50 ml"),
            price_text: String::new(),
            price: 219000.0,
            ..Default::default()
        },
        Product {
            name: String::from("light blue femme edp 100 ml"),
            price_text: String::new(),
            price: 311400.0,
            ..Default::default()
        },
        Product {
            name: String::from("light blue homme edp 100 ml"),
            price_text: String::new(),
            price: 279900.0,
            ..Default::default()
        },
        Product {
            name: String::from("kevin dubai edp 100 ml"),
            price_text: String::new(),
            price: 40050.0,
            ..Default::default()
        },
        Product {
            name: String::from("set paula aura edt 60 + deo"),
            price_text: String::new(),
            price: 38250.0,
            ..Default::default()
        },
        Product {
            name: String::from("set wanted forever Elixir parfum 100 ml"),
            price_text: String::new(),
            price: 283500.0,
            ..Default::default()
        },
        Product {
            name: String::from("212 Vip edp 80 ml"),
            price_text: String::new(),
            price: 278100.0,
            ..Default::default()
        },
        Product {
            name: String::from("212 Vip Rose Ny Rodeo edp 80 ml"),
            price_text: String::new(),
            price: 323100.0,
            ..Default::default()
        },
        Product {
            name: String::from("ch men swing edp 100 ml"),
            price_text: String::new(),
            price: 225900.0,
            ..Default::default()
        },
        Product {
            name: String::from("ch women swing edp 100 ml"),
            price_text: String::new(),
            price: 261000.0,
            ..Default::default()
        },
        Product {
            name: String::from("the scent elixir parfum 100 ml"),
            price_text: String::new(),
            price: 299250.0,
            ..Default::default()
        },
        Product {
            name: String::from("set the most wanted parfum 100 ml"),
            price_text: String::new(),
            price: 259200.0,
            ..Default::default()
        },
        Product {
            name: String::from("jadore intense parfum 100 ml"),
            price_text: String::new(),
            price: 319050.0,
            ..Default::default()
        },
        Product {
            name: String::from("dylan blush pink edp 30 ml"),
            price_text: String::new(),
            price: 175500.0,
            ..Default::default()
        },
        Product {
            name: String::from("dylan blush pink edp 50 ml"),
            price_text: String::new(),
            price: 254700.0,
            ..Default::default()
        },
        Product {
            name: String::from("dylan blush pink edp 100 ml"),
            price_text: String::new(),
            price: 295200.0,
            ..Default::default()
        },
        Product {
            name: String::from("forever wanted absolu parfum 50 ml"),
            price_text: String::new(),
            price: 238500.0,
            ..Default::default()
        },
        Product {
            name: String::from("the one parfum 100 ml"),
            price_text: String::new(),
            price: 327600.0,
            ..Default::default()
        },
        Product {
            name: String::from("24/7 electric edp 100 ml"),
            price_text: String::new(),
            price: 99810.0,
            ..Default::default()
        },
        Product {
            name: String::from("forever wanted absolu parfum 100 ml"),
            price_text: String::new(),
            price: 297900.0,
            ..Default::default()
        },
        Product {
            name: String::from("un jardin sous la mer edt 100 ml"),
            price_text: String::new(),
            price: 305100.0,
            ..Default::default()
        },
        Product {
            name: String::from("solo sport edt 125 ml"),
            price_text: String::new(),
            price: 243000.0,
            ..Default::default()
        },
        Product {
            name: String::from("fierce cologne edc 100 ml"),
            price_text: String::new(),
            price: 141300.0,
            ..Default::default()
        },
        Product {
            name: String::from("set flowerbomb edp 100 ml"),
            price_text: String::new(),
            price: 325800.0,
            ..Default::default()
        },
        Product {
            name: String::from("hidden fantasy edp 100 ml"),
            price_text: String::new(),
            price: 143991.0,
            ..Default::default()
        },
        Product {
            name: String::from("set 212 men edt 100 + deo + travel"),
            price_text: String::new(),
            price: 233910.0,
            ..Default::default()
        },
        Product {
            name: String::from("212 men parfum 100 ml"),
            price_text: String::new(),
            price: 253575.0,
            ..Default::default()
        },
        Product {
            name: String::from("la panthere edt 75 ml"),
            price_text: String::new(),
            price: 370350.0,
            ..Default::default()
        },
        Product {
            name: String::from("212 sexy edt 100 ml"),
            price_text: String::new(),
            price: 194000.0,
            ..Default::default()
        },
        Product {
            name: String::from("krizia uomo edt 100 ml"),
            price_text: String::new(),
            price: 76500.0,
            ..Default::default()
        },
        Product {
            name: String::from("lolita le parfum edp 100 ml"),
            price_text: String::new(),
            price: 212400.0,
            ..Default::default()
        },
        Product {
            name: String::from("Agua de Bambu edt 120 ml"),
            price_text: String::new(),
            price: 67941.0,
            ..Default::default()
        },
        Product {
            name: String::from("la panthere legere edp 50 ml"),
            price_text: String::new(),
            price: 303300.0,
            ..Default::default()
        },
        Product {
            name: String::from("la panthere legere edp 75 ml"),
            price_text: String::new(),
            price: 396900.0,
            ..Default::default()
        },
        Product {
            name: String::from("madame gres edp 100 ml"),
            price_text: String::new(),
            price: 159300.0,
            ..Default::default()
        },
        Product {
            name: String::from("petite robe noire hippie chic edp 50 ml"),
            price_text: String::new(),
            price: 234000.0,
            ..Default::default()
        },
        Product {
            name: String::from("alyssa edp 100 ml"),
            price_text: String::new(),
            price: 49000.0,
            ..Default::default()
        },
        Product {
            name: String::from("faris edp 100 ml"),
            price_text: String::new(),
            price: 49000.0,
            ..Default::default()
        },
        Product {
            name: String::from("prestige status edp 80 ml"),
            price_text: String::new(),
            price: 69000.0,
            ..Default::default()
        },
        Product {
            name: String::from("prestige esteem edp 80 ml"),
            price_text: String::new(),
            price: 69000.0,
            ..Default::default()
        },
        Product {
            name: String::from("prestige honor edp 80 ml"),
            price_text: String::new(),
            price: 69000.0,
            ..Default::default()
        },
        Product {
            name: String::from("addiction edp 100 ml"),
            price_text: String::new(),
            price: 49000.0,
            ..Default::default()
        },
        Product {
            name: String::from("shams edition ambre edp 100 ml"),
            price_text: String::new(),
            price: 69000.0,
            ..Default::default()
        },
        Product {
            name: String::from("fusion accord edp 85 ml"),
            price_text: String::new(),
            price: 69000.0,
            ..Default::default()
        },
        Product {
            name: String::from("flower eau de lumiere edt 30 ml"),
            price_text: String::new(),
            price: 159300.0,
            ..Default::default()
        },
        Product {
            name: String::from("guilty femme edp 90 ml"),
            price_text: String::new(),
            price: 288000.0,
            ..Default::default()
        },
        Product {
            name: String::from("ck one edt 100 ml"),
            price_text: String::new(),
            price: 179955.0,
            ..Default::default()
        },
        Product {
            name: String::from("gentleman society sport edp 100 ml"),
            price_text: String::new(),
            price: 251100.0,
            ..Default::default()
        },
        Product {
            name: String::from("jadore parfum d'eau edp 50 ml"),
            price_text: String::new(),
            price: 254250.0,
            ..Default::default()
        },
        Product {
            name: String::from("bloom edt 50 ml"),
            price_text: String::new(),
            price: 180000.0,
            ..Default::default()
        },
        Product {
            name: String::from("eau de rochas edt 50 ml"),
            price_text: String::new(),
            price: 164700.0,
            ..Default::default()
        },
        Product {
            name: String::from("power of you edp 50 ml"),
            price_text: String::new(),
            price: 197100.0,
            ..Default::default()
        },
        Product {
            name: String::from("power of you edp 90 ml"),
            price_text: String::new(),
            price: 314100.0,
            ..Default::default()
        },
        Product {
            name: String::from("fame in love parfum elixir 80 ml"),
            price_text: String::new(),
            price: 279450.0,
            ..Default::default()
        },
        Product {
            name: String::from("la petite robe noire intense edp 100 ml"),
            price_text: String::new(),
            price: 306000.0,
            ..Default::default()
        },
        Product {
            name: String::from("ysl y le parfum 60 ml"),
            price_text: String::new(),
            price: 269100.0,
            ..Default::default()
        },
        Product {
            name: String::from("good girl dot drama edp 80 ml"),
            price_text: String::new(),
            price: 290421.0,
            ..Default::default()
        },
        Product {
            name: String::from("set kenzo homme intense edt 110 ml"),
            price_text: String::new(),
            price: 232200.0,
            ..Default::default()
        },
        Product {
            name: String::from("flower le rouge edp 100 ml"),
            price_text: String::new(),
            price: 269100.0,
            ..Default::default()
        },
        Product {
            name: String::from("azzaro l'eau edt 50 ml"),
            price_text: String::new(),
            price: 154800.0,
            ..Default::default()
        },
        Product {
            name: String::from("aqua essenziale colonia edt 50 ml"),
            price_text: String::new(),
            price: 126900.0,
            ..Default::default()
        },
        Product {
            name: String::from("miss dior eau fraiche edt 50 ml"),
            price_text: String::new(),
            price: 217710.0,
            ..Default::default()
        },
        Product {
            name: String::from("solarissimo edt 75 ml"),
            price_text: String::new(),
            price: 148500.0,
            ..Default::default()
        },
        Product {
            name: String::from("First Instinct edt 50 ml"),
            price_text: String::new(),
            price: 109800.0,
            ..Default::default()
        },
        Product {
            name: String::from("aqua allegoria rosa rossa edt 75 ml"),
            price_text: String::new(),
            price: 194400.0,
            ..Default::default()
        },
        Product {
            name: String::from("aqua allegoria pera granita edt 75 ml"),
            price_text: String::new(),
            price: 194400.0,
            ..Default::default()
        },
        Product {
            name: String::from("aqua allegoria mandarine basilisc edt 75 ml"),
            price_text: String::new(),
            price: 194400.0,
            ..Default::default()
        },
        Product {
            name: String::from("aqua allegoria florabloom edt 75 ml"),
            price_text: String::new(),
            price: 194400.0,
            ..Default::default()
        },
    ]
}

#[test]
fn extracts_all_products_from_todoslosperfumes_fixture() {
    common::assert_fixture(
        "tests/fixtures/todoslosperfumes.html",
        &products(),
        "products",
    );
}
