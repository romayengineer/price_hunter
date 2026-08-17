#![allow(clippy::cognitive_complexity)]

use super::detect_grid;
use super::extract::{find_size_in_text, has_size, has_trailing_bare_number};
use super::prices::{classify_div, number_tokens, parse_price};

fn grid_page() -> String {
    r#"
        <html><body>
          <div id="wrapper">
            <h1>Shop</h1>
            <div class="product-grid">
              <div class="card"><a href="/p1">Widget A</a><span class="price">$12.99</span></div>
              <div class="card"><a href="/p2">Widget B</a><span class="price">19,95</span></div>
              <div class="card"><a href="/p3">Widget C</a><span class="price">1.234,56</span></div>
              <div class="card"><a href="/p4">Widget D</a><span class="price">1,299.00</span></div>
            </div>
            <div id="footer">Contact us</div>
          </div>
        </body></html>
        "#
    .to_string()
}

#[test]
fn detects_grid_with_mixed_formats() {
    let detection = detect_grid(&grid_page()).expect("grid should be detected");
    assert_eq!(detection.container.classes, vec!["product-grid"]);
    assert_eq!(detection.products.len(), 4);
    assert_eq!(detection.products[0].name, "Widget A");
    assert_eq!(detection.products[0].price, 12.99);
    assert_eq!(detection.products[1].price, 19.95);
    assert_eq!(detection.products[2].price, 1234.56);
    assert_eq!(detection.products[3].price, 1299.0);
}

#[test]
fn no_grid_without_prices() {
    let html = "<html><body><div><p>No prices here.</p></div></body></html>";
    assert!(detect_grid(html).is_none());
}

#[test]
fn single_price_div_is_not_a_grid() {
    let html = "<html><body><div class=\"card\"><span>12,99</span></div></body></html>";
    assert!(detect_grid(html).is_none());
}

#[test]
fn nested_wrapper_divs_inside_cards() {
    let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card"><div class="inner"><span class="price">10,00</span></div></div>
            <div class="card"><div class="inner"><span class="price">20,00</span></div></div>
            <div class="card"><div class="inner"><span class="price">30,00</span></div></div>
          </div>
        </body></html>
        "#;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(detection.container.classes, vec!["product-grid"]);
    assert_eq!(detection.products.len(), 3);
}

#[test]
fn picks_larger_of_two_grids() {
    let html = r#"
        <html><body>
          <div class="small-grid">
            <div class="card"><a>Alpha</a><span>1,00</span></div>
            <div class="card"><a>Beta</a><span>2,00</span></div>
          </div>
          <div class="big-grid">
            <div class="card"><a>One</a><span>1,00</span></div>
            <div class="card"><a>Two</a><span>2,00</span></div>
            <div class="card"><a>Three</a><span>3,00</span></div>
            <div class="card"><a>Four</a><span>4,00</span></div>
            <div class="card"><a>Five</a><span>5,00</span></div>
          </div>
        </body></html>
        "#;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(detection.container.classes, vec!["big-grid"]);
    assert_eq!(detection.products.len(), 5);
}

#[test]
fn bare_integer_price_is_detected() {
    let html = r#"
        <html><body>
          <div class="grid">
            <div class="card"><div class="price"><span>499</span></div></div>
            <div class="card"><div class="price"><span>899</span></div></div>
          </div>
        </body></html>
        "#;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(detection.products.len(), 2);
    assert_eq!(detection.products[0].price, 499.0);
}

#[test]
fn thousands_only_price() {
    let html = r#"
        <html><body>
          <div class="grid">
            <div class="card"><div class="price"><span>1,234</span></div></div>
            <div class="card"><div class="price"><span>5.678</span></div></div>
          </div>
        </body></html>
        "#;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(detection.products[0].price, 1234.0);
    assert_eq!(detection.products[1].price, 5678.0);
}

#[test]
fn parse_price_woocommerce_thousands() {
    assert_eq!(parse_price("8.190"), Some(8190.0));
    assert_eq!(parse_price("12.990"), Some(12990.0));
    assert_eq!(parse_price("3.450"), Some(3450.0));
    assert_eq!(parse_price("8,190"), Some(8190.0));
}

#[test]
fn detects_woocommerce_price_grid() {
    let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card"><a href="/a">Alpha</a><bdi><span class="woocommerce-Price-currencySymbol">$</span>8.190</bdi></div>
            <div class="card"><a href="/b">Beta</a><bdi><span class="woocommerce-Price-currencySymbol">$</span>12.990</bdi></div>
            <div class="card"><a href="/c">Gamma</a><bdi><span class="woocommerce-Price-currencySymbol">$</span>3.450</bdi></div>
          </div>
        </body></html>
        "#;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(detection.container.classes, vec!["product-grid"]);
    assert_eq!(detection.products.len(), 3);
    assert_eq!(detection.products[0].name, "Alpha");
    assert_eq!(detection.products[1].name, "Beta");
    assert_eq!(detection.products[2].name, "Gamma");
    assert_eq!(detection.products[0].price, 8190.0);
    assert_eq!(detection.products[1].price, 12990.0);
    assert_eq!(detection.products[2].price, 3450.0);
    assert_eq!(detection.products[0].price_text, "8.190");
    assert_eq!(detection.products[1].price_text, "12.990");
    assert_eq!(detection.products[2].price_text, "3.450");
}

#[test]
fn currency_symbol_alone_is_not_a_price() {
    assert!(number_tokens("$").is_empty());
    let price = classify_div("$8.190").expect("price should be found");
    assert_eq!(price.len(), 1);
    assert_eq!(price[0].value, 8190.0);
    assert_eq!(price[0].text, "8.190");
}

#[test]
fn prestashop_prefers_itemprop_price() {
    let html = r#"
        <html><body>
          <div class="products row">
            <article class="product-miniature">
              <div class="product-description">
                <h2 class="product-title"><a href="/a">Light Blue Homme EDP 50</a></h2>
                <div class="product-price-and-shipping">
                  <span itemprop="price" class="price cod"><span>$242.100</span></span>
                  <span class="regular-price">$269.000</span>
                  <span class="discount-percentage discount-product">-10%</span>
                </div>
              </div>
            </article>
            <article class="product-miniature">
              <div class="product-description">
                <h2 class="product-title"><a href="/b">Bottled Beyond EDT 50</a></h2>
                <div class="product-price-and-shipping">
                  <span itemprop="price" class="price cod"><span>$219.000</span></span>
                  <span class="regular-price">$249.000</span>
                  <span class="discount-amount discount-product">-$30.000</span>
                </div>
              </div>
            </article>
          </div>
        </body></html>
        "#;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(detection.container.classes, vec!["products", "row"]);
    assert_eq!(detection.products.len(), 2);
    assert_eq!(detection.products[0].name, "Light Blue Homme EDP 50 ml");
    assert_eq!(detection.products[0].price, 242100.0);
    assert_eq!(detection.products[0].price_text, "242.100");
    assert_eq!(detection.products[1].name, "Bottled Beyond EDT 50 ml");
    assert_eq!(detection.products[1].price, 219000.0);
    assert_eq!(detection.products[1].price_text, "219.000");
}

#[test]
fn magento_ul_list_splits_cards_and_prefers_final_price() {
    let html = r#"
        <html><body>
          <div class="products wrapper mode-grid products-grid">
            <ul role="list">
              <li>
                <form class="product_addtocart_form">
                  <strong class="product brand"><a class="product-item-link" href="/brands/x">RABANNE</a></strong>
                  <a class="product-item-link" data-role="product-item-name" href="/a">FAME COUTURE EDP 80ML</a>
                  <div class="price-box price-final_price">
                    <span class="price-wrapper price-including-tax" data-price-type="finalPrice"><span class="price">$&nbsp;264.000</span></span>
                    <span class="price-wrapper price-excluding-tax" data-price-type="basePrice"><span class="price">$&nbsp;218.182</span></span>
                  </div>
                  <div class="product-installments"><span class="amount">$&nbsp;22.000</span></div>
                </form>
              </li>
              <li>
                <form class="product_addtocart_form">
                  <strong class="product brand"><a class="product-item-link" href="/brands/y">CAROLINA HERRERA</a></strong>
                  <a class="product-item-link" data-role="product-item-name" href="/b">212 SEXY MEN EDT 100ML</a>
                  <div class="price-box price-final_price">
                    <span class="old-price"><span class="price-wrapper price-including-tax" data-price-type="oldPrice"><span class="price">$&nbsp;225.000</span></span></span>
                    <span class="normal-price"><span class="price-wrapper price-including-tax" data-price-type="finalPrice"><span class="price">$&nbsp;165.000</span></span></span>
                    <span class="price-wrapper price-excluding-tax" data-price-type="basePrice"><span class="price">$&nbsp;165.000</span></span>
                  </div>
                </form>
              </li>
            </ul>
          </div>
        </body></html>
        "#;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(
        detection.container.classes,
        vec!["mode-grid", "products", "products-grid", "wrapper"]
    );
    assert_eq!(detection.products.len(), 2);
    assert_eq!(detection.products[0].name, "RABANNE FAME COUTURE EDP 80ML");
    assert_eq!(detection.products[0].price, 264000.0);
    assert_eq!(detection.products[0].price_text, "264.000");
    assert_eq!(
        detection.products[1].name,
        "CAROLINA HERRERA 212 SEXY MEN EDT 100ML"
    );
    assert_eq!(detection.products[1].price, 165000.0);
    assert_eq!(detection.products[1].price_text, "165.000");
}

#[test]
fn vtex_brand_container_is_prepended_when_missing_from_name() {
    let html = r##"
        <html><body>
          <div class="vtex-search-result-3-x-gallery">
            <div class="vtex-product-summary-2-x-containerNormal">
              <a class="vtex-product-summary-2-x-clearLink" aria-label="Gold Fresh Couture EDP">
                <div class="vtex-product-summary-2-x-productBrandContainer"><span class="vtex-product-summary-2-x-productBrandName">Moschino</span></div>
                <h3 class="vtex-product-summary-2-x-nameContainer">
                  <span class="vtex-product-summary-2-x-productBrand vtex-product-summary-2-x-brandName t-body">Gold Fresh Couture EDP</span>
                </h3>
                <span class="price">$ 95.400</span>
              </a>
            </div>
            <div class="vtex-product-summary-2-x-containerNormal">
              <a class="vtex-product-summary-2-x-clearLink" aria-label="Adidas Vibes Smooth Pace EDP">
                <div class="vtex-product-summary-2-x-productBrandContainer"><span class="vtex-product-summary-2-x-productBrandName">Adidas</span></div>
                <h3 class="vtex-product-summary-2-x-nameContainer">
                  <span class="vtex-product-summary-2-x-productBrand vtex-product-summary-2-x-brandName t-body">Adidas Vibes Smooth Pace EDP</span>
                </h3>
                <span class="price">$ 21.450</span>
              </a>
            </div>
            <div class="vtex-product-summary-2-x-containerNormal">
              <a class="vtex-product-summary-2-x-clearLink" aria-label="Dylan Blush Pink EDP 100 ml">
                <h3 class="vtex-product-summary-2-x-nameContainer">
                  <span class="vtex-product-summary-2-x-productBrand vtex-product-summary-2-x-brandName t-body">Dylan Blush Pink EDP 100 ml</span>
                </h3>
                <span class="price">$ 328.000</span>
              </a>
            </div>
          </div>
        </body></html>
        "##;
    let detection = detect_grid(html).expect("grid should be detected");
    // Brand from the VTEX productBrandName element is prepended.
    assert_eq!(detection.products[0].name, "Moschino Gold Fresh Couture EDP");
    // Brand already present in the name is not duplicated.
    assert_eq!(detection.products[1].name, "Adidas Vibes Smooth Pace EDP");
    // No brand element (only the productBrand/brandName name span) -> unchanged.
    assert_eq!(detection.products[2].name, "Dylan Blush Pink EDP 100 ml");
}

#[test]
fn magento_brand_strong_is_prepended_when_missing_from_name() {
    let html = r##"
        <html><body>
          <div class="products wrapper mode-grid products-grid">
            <ul role="list">
              <li>
                <strong class="product brand product-item-brand"><a class="product-item-link" href="/brands/x">RABANNE</a></strong>
                <a class="product-item-link" data-role="product-item-name" href="/a">FAME COUTURE EDP 80ML</a>
                <div class="price-box"><span data-price-type="finalPrice"><span class="price">$ 264.000</span></span></div>
              </li>
              <li>
                <strong class="product brand product-item-brand"><a class="product-item-link" href="/brands/y">CALVIN KLEIN</a></strong>
                <a class="product-item-link" data-role="product-item-name" href="/b">CK ONE EDT 100ML</a>
                <div class="price-box"><span data-price-type="finalPrice"><span class="price">$ 179.955</span></span></div>
              </li>
            </ul>
          </div>
        </body></html>
        "##;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(detection.products[0].name, "RABANNE FAME COUTURE EDP 80ML");
    assert_eq!(
        detection.products[1].name,
        "CALVIN KLEIN CK ONE EDT 100ML"
    );
}

#[test]
fn two_prices_card_picks_current_price() {
    let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card"><a href="/x">Kappa</a><bdi><span class="woocommerce-Price-currencySymbol">€</span>12,99</bdi></div>
            <div class="card"><a href="/y">Lambda</a><bdi><span class="woocommerce-Price-currencySymbol">€</span>24,50</bdi></div>
          </div>
        </body></html>
        "#;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(detection.products.len(), 2);
    assert_eq!(detection.products[0].price, 12.99);
    assert_eq!(detection.products[0].price_text, "12,99");
    assert_eq!(detection.products[1].price, 24.5);
}

#[test]
fn captures_product_url_images_and_currency() {
    let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card">
              <a data-role="product-item-name" href="/perfumes/alpha">Alpha EDP 50</a>
              <img src="/img/alpha-1.jpg" alt="Alpha EDP 50">
              <img src="/img/alpha-1.jpg" alt="Alpha EDP 50">
              <img src="/img/alpha-2.jpg" alt="Alpha EDP 50">
              <span class="price">$&nbsp;242.100</span>
            </div>
            <div class="card">
              <a data-role="product-item-name" href="/perfumes/beta">Beta EDP 50</a>
              <img src="/img/beta.jpg" alt="Beta EDP 50">
              <span class="price">AR$&nbsp;99.900</span>
            </div>
          </div>
        </body></html>
        "#;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(detection.products.len(), 2);
    assert_eq!(
        detection.products[0].url.as_deref(),
        Some("/perfumes/alpha")
    );
    assert_eq!(
        detection.products[0].images,
        vec![
            "/img/alpha-1.jpg".to_string(),
            "/img/alpha-2.jpg".to_string()
        ]
    );
    assert_eq!(detection.products[1].url.as_deref(), Some("/perfumes/beta"));
    assert_eq!(
        detection.products[1].images,
        vec!["/img/beta.jpg".to_string()]
    );
    assert_eq!(detection.products[1].currency.as_deref(), Some("ARS"));
}

#[test]
fn bare_dollar_does_not_force_a_currency() {
    let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card"><a href="/a">Alpha</a><span class="price">$ 12.990</span></div>
            <div class="card"><a href="/b">Beta</a><span class="price">$ 8.190</span></div>
          </div>
        </body></html>
        "#;
    let detection = detect_grid(html).expect("grid should be detected");
    assert!(detection.products.iter().all(|p| p.currency.is_none()));
}

#[test]
fn product_link_skips_placeholder_anchors() {
    // compreahora-style card: icon/button anchors come first in the card
    // and must not be picked as the product URL.
    let html = r##"
        <html><body>
          <div class="product-grid">
            <div class="card">
              <a href="https://www.compreahora.com.ar/categoria/perfumeria#" class="shopping-list-icon"><img alt="Axe Gold"></a>
              <a href="javascript:void(0)"><img alt="Axe Gold"></a>
              <a href="#"><span class="icon"></span></a>
              <h3><a href="/producto/desodorante-axe-gold-vainilla-en-aerosol-150-ml">Desodorante Axe Gold vainilla en aerosol 150 ml</a></h3>
              <span class="price">$ 3.744,05</span>
            </div>
            <div class="card">
              <a href="https://www.compreahora.com.ar/categoria/perfumeria#" class="shopping-list-icon"><img alt="Axe Musk"></a>
              <h3><a href="/producto/desodorante-para-hombre-axe-musk-musk-en-aerosol-150-ml">Desodorante para hombre Axe Musk musk en aerosol 150 ml</a></h3>
              <span class="price">$ 3.744,05</span>
            </div>
            <div class="card">
              <a href="javascript:void(0)"><img alt="Dove"></a>
              <h3><a href="/producto/antitranspirante-pomelo-1-4-crema-humectante-dove-en-aerosol-150-ml">Antitranspirante pomelo 1/4 crema humectante Dove en aerosol 150 ml</a></h3>
              <span class="price">$ 4.564,91</span>
            </div>
          </div>
        </body></html>
        "##;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(
        detection.products[0].url.as_deref(),
        Some("/producto/desodorante-axe-gold-vainilla-en-aerosol-150-ml")
    );
    assert_eq!(
        detection.products[1].url.as_deref(),
        Some("/producto/desodorante-para-hombre-axe-musk-musk-en-aerosol-150-ml")
    );
}

#[test]
fn size_helpers_recognize_units() {
    assert_eq!(find_size_in_text("Gold Fresh Couture EDP"), None);
    assert_eq!(find_size_in_text("Crystal Emerald EDP"), None);
    assert_eq!(
        find_size_in_text("Dylan Blush Pink EDP 100 ml"),
        Some("100 ml".into())
    );
    assert_eq!(find_size_in_text("light blue homme edp 50"), None);
    assert_eq!(
        find_size_in_text("PAULVIC WOMAN X50ML"),
        Some("50ML".into())
    );
    assert_eq!(find_size_in_text("132 g"), Some("132 g".into()));
    assert_eq!(find_size_in_text("100 Ml"), Some("100 Ml".into()));
    assert_eq!(find_size_in_text("Promo 100ml"), Some("100ml".into()));
    assert_eq!(
        find_size_in_text("fresh-gold-edp-precio-promocional-100ml/p"),
        Some("100ml".into())
    );
    assert!(has_size("Blue Jeans EDT 75 ml"));
    assert!(!has_size("Funny EDT Ed. Limitada"));
    assert!(has_trailing_bare_number("light blue homme edp 50"));
    assert!(!has_trailing_bare_number("One Million EDT"));
}

#[test]
fn size_from_sku_selector_prefers_selected() {
    let html = r##"
        <html><body>
          <div class="product-grid">
            <div class="card">
              <h3><a href="/crystal-emerald">Crystal Emerald EDP</a></h3>
              <div class="skuSelectorContainer">
                <div class="skuSelectorItem skuSelectorItem--50-ml"><span class="valueWrapper">50 ml</span></div>
                <div class="skuSelectorItem skuSelectorItem--90-ml skuSelectorItem--selected"><span class="valueWrapper">90 ml</span></div>
              </div>
              <span class="price">$ 328.000</span>
            </div>
            <div class="card">
              <h3><a href="/dylan-blush">Dylan Blush Pink EDP 100 ml</a></h3>
              <span class="price">$ 328.000</span>
            </div>
            <div class="card">
              <h3><a href="/blue-jeans">Blue Jeans EDT 75 ml</a></h3>
              <span class="price">$ 79.990</span>
            </div>
          </div>
        </body></html>
        "##;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(detection.products[0].name, "Crystal Emerald EDP 90 ml");
}

#[test]
fn size_from_url_when_no_sku_selector() {
    let html = r##"
        <html><body>
          <div class="product-grid">
            <div class="card">
              <h3><a href="/funny-edt-100ml-ed-limitada-1/p">Funny EDT Ed. Limitada</a></h3>
              <span class="price">$ 94.340</span>
            </div>
            <div class="card">
              <h3><a href="/plain-product/p">Plain Product</a></h3>
              <span class="price">$ 50.000</span>
            </div>
            <div class="card">
              <h3><a href="/fresh-gold-100ml/p">Fresh Gold EDP 100 ml</a></h3>
              <span class="price">$ 95.400</span>
            </div>
          </div>
        </body></html>
        "##;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(detection.products[0].name, "Funny EDT Ed. Limitada 100ml");
    // No size in URL and no trailing number -> name unchanged.
    assert_eq!(detection.products[1].name, "Plain Product");
}

#[test]
fn trailing_bare_number_gets_ml() {
    let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card"><a href="/p/light-blue-homme-edp-50.html">light blue homme edp
              50</a><span class="price">$242.100</span></div>
            <div class="card"><a href="/p/one-million.html">One Million EDT</a><span class="price">$266.901</span></div>
            <div class="card"><a href="/p/paula-aura-edt-100.html">paula aura edt 100</a><span class="price">$39.060</span></div>
          </div>
        </body></html>
        "#;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(detection.products[0].name, "light blue homme edp 50 ml");
    assert_eq!(detection.products[1].name, "One Million EDT");
    assert_eq!(detection.products[2].name, "paula aura edt 100 ml");
}

#[test]
fn existing_size_is_left_untouched() {
    let html = r#"
        <html><body>
          <div class="product-grid">
            <div class="card"><a href="/p/dylan">Dylan Blush Pink EDP 100 ml + Neceser</a><span class="price">$328.000</span></div>
            <div class="card"><a href="/p/axe">Desodorante Axe Gold 150 ml</a><span class="price">$3.744</span></div>
            <div class="card"><a href="/p/rexona">Rexona 132 g</a><span class="price">$4.489</span></div>
          </div>
        </body></html>
        "#;
    let detection = detect_grid(html).expect("grid should be detected");
    assert_eq!(
        detection.products[0].name,
        "Dylan Blush Pink EDP 100 ml + Neceser"
    );
    assert_eq!(detection.products[1].name, "Desodorante Axe Gold 150 ml");
    assert_eq!(detection.products[2].name, "Rexona 132 g");
}
