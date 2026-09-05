//! Fixed 30-product catalog used as ground truth for generated grid tests.
//! Names/prices are derived from real fixtures (pigmento / beauty24) to stay
//! realistic and stable.

use price_hunter_core::detect::Product;

pub fn fixed_products() -> Vec<Product> {
    vec![
        Product { name: "Moschino Gold Fresh Couture EDP 100 Ml".into(), price: 95400.0, ..Default::default() },
        Product { name: "Moschino Funny EDT 100 Ml".into(), price: 89000.0, ..Default::default() },
        Product { name: "Moschino Fresh Couture EDT 100 Ml".into(), price: 94900.0, ..Default::default() },
        Product { name: "Adidas Vibes Smooth Pace EDP Unisex 100 Ml".into(), price: 21450.0, ..Default::default() },
        Product { name: "Moschino Pink Fresh Couture EDT 100 Ml".into(), price: 89000.0, ..Default::default() },
        Product { name: "Versace Red Jeans EDT 75 Ml".into(), price: 79990.0, ..Default::default() },
        Product { name: "Calvin Klein Defy Men EDP 200 Ml".into(), price: 187500.0, ..Default::default() },
        Product { name: "Ted Lapidus Orissima EDP 30 Ml".into(), price: 56490.0, ..Default::default() },
        Product { name: "Azzaro Sport EDT 100 Ml".into(), price: 111000.0, ..Default::default() },
        Product { name: "Ted Lapidus Rumba Fever EDT 100 Ml".into(), price: 90900.0, ..Default::default() },
        Product { name: "Elizabeth Arden Green Tea Lavender EDT 100 Ml".into(), price: 52030.0, ..Default::default() },
        Product { name: "Bensimon Blue Night EDP 200 Ml".into(), price: 48594.0, ..Default::default() },
        Product { name: "Carolina Herrera 212 Nyc EDT 30 Ml".into(), price: 102000.0, ..Default::default() },
        Product { name: "Rabanne One Million EDT 200 Ml".into(), price: 266901.0, ..Default::default() },
        Product { name: "Carolina Herrera 212 Vip Black Men EDP 200 Ml".into(), price: 264063.0, ..Default::default() },
        Product { name: "Dylan Blush Pink EDP 100 ml + Neceser".into(), price: 328000.0, ..Default::default() },
        Product { name: "Crystal Emerald EDP 90 ml".into(), price: 328000.0, ..Default::default() },
        Product { name: "Blue Jeans EDT 75 ml".into(), price: 79990.0, ..Default::default() },
        Product { name: "Fresh Gold EDP 100 ml".into(), price: 95400.0, ..Default::default() },
        Product { name: "Funny EDT Ed. Limitada 100ml".into(), price: 94340.0, ..Default::default() },
        Product { name: "Light Blue Homme EDP 50 ml".into(), price: 242100.0, ..Default::default() },
        Product { name: "Bottled Beyond EDT 50 ml".into(), price: 219000.0, ..Default::default() },
        Product { name: "Rabanne Fame Couture EDP 80 ml".into(), price: 264000.0, ..Default::default() },
        Product { name: "Carolina Herrera 212 Sexy Men EDT 100 ml".into(), price: 165000.0, ..Default::default() },
        Product { name: "Antitranspirante pomelo 1/4 crema humectante Dove en aerosol 150 ml".into(), price: 4564.91, ..Default::default() },
        Product { name: "Desodorante Axe Gold vainilla en aerosol 150 ml".into(), price: 3744.05, ..Default::default() },
        Product { name: "Repelente de insectos Livopen sports en aerosol 132 g".into(), price: 4489.27, ..Default::default() },
        Product { name: "Nautica Voyage EDT 100 Ml".into(), price: 38624.0, ..Default::default() },
        Product { name: "Davidoff Cool Water Men EDT 75 Ml".into(), price: 137214.0, ..Default::default() },
        Product { name: "Lancome La Vie Est Belle Intensement EDP 100 Ml".into(), price: 239000.0, ..Default::default() },
    ]
}
