//! Generated-grid resilience tests via Askama template expansion.
//! 50 hand-written parametric templates under `tests/templates/` (isolated).
//! Each template is rendered with the fixed 30-product catalog (pre-formatted
//! price_text/href/brand) via `tests/support/renderer.rs`. Autoescape is ON.
//! Fixed 1:1 mapping: `generated_variant_07` → `07_*.html`. Detector + parse
//! validation: `Html::parse_document` must succeed and `detect_grid` must find
//! 30 products with exact `name+price`. On failure, HTML is dumped to
//! `target/tmp/askama-{idx}.html` + `diagnose_containers` printed.

mod support;

use price_hunter_core::detect::{Detection, diagnose_containers, detect_grid};
use scraper::Html;
use support::catalog::fixed_products;
use support::renderer::{render_template_by_idx, template_name};

macro_rules! gen_test {
    ($name:ident, $idx:expr) => {
        #[test]
        fn $name() {
            let products = fixed_products();
            let html = render_template_by_idx($idx, &products);
            // Detector + parse validation (per Q&A)
            let parse_ok = Html::parse_document(&html);
            // scraper parse always succeeds, but ensure at least html tree has nodes
            assert!(
                parse_ok.tree.nodes().count() > 0,
                "variant {} {} produced unparsable HTML",
                $idx,
                template_name($idx)
            );
            let detection = detect_grid(&html);
            if detection.is_none() {
                let diag = diagnose_containers(&html);
                dump_and_panic($idx, &html, &diag, "detect_grid returned None");
            }
            let det = detection.unwrap();
            if det.products.len() != products.len() {
                let diag = diagnose_containers(&html);
                dump_and_panic(
                    $idx,
                    &html,
                    &diag,
                    &format!(
                        "expected {} products, got {}: {:?}",
                        products.len(),
                        det.products.len(),
                        det.products.iter().map(|p| (&p.name, p.price)).collect::<Vec<_>>()
                    ),
                );
            }
            for exp in &products {
                let found = det
                    .products
                    .iter()
                    .any(|p| p.name == exp.name && (p.price - exp.price).abs() < 0.01);
                if !found {
                    let diag = diagnose_containers(&html);
                    dump_and_panic(
                        $idx,
                        &html,
                        &diag,
                        &format!("expected product not found: {:?}", exp),
                    );
                }
            }
        }
    };
}

fn dump_and_panic(
    idx: usize,
    html: &str,
    diag: &[price_hunter_core::detect::ContainerCandidate],
    msg: &str,
) -> ! {
    let dir = "target/tmp";
    let _ = std::fs::create_dir_all(dir);
    let path = format!("{dir}/askama-{:02}-{}.html", idx, template_name(idx));
    let _ = std::fs::write(&path, html);
    let diag_str = diag
        .iter()
        .map(|c| {
            format!(
                "  classes={:?} id={:?} prices={} divs={} density={:.4} selected={}",
                c.classes, c.id, c.price_count, c.div_count, c.density, c.selected
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let excerpt = if html.len() > 2000 { &html[..2000] } else { html };
    panic!(
        "askama variant {} {} failed: {msg}\ndumped to {path}\ndiagnose_containers:\n{diag_str}\nhtml excerpt (first 2k):\n{excerpt}",
        idx,
        template_name(idx)
    );
}

// 50 parameterized tests – fixed mapping 00..49
gen_test!(generated_variant_00, 0);
gen_test!(generated_variant_01, 1);
gen_test!(generated_variant_02, 2);
gen_test!(generated_variant_03, 3);
gen_test!(generated_variant_04, 4);
gen_test!(generated_variant_05, 5);
gen_test!(generated_variant_06, 6);
gen_test!(generated_variant_07, 7);
gen_test!(generated_variant_08, 8);
gen_test!(generated_variant_09, 9);
gen_test!(generated_variant_10, 10);
gen_test!(generated_variant_11, 11);
gen_test!(generated_variant_12, 12);
gen_test!(generated_variant_13, 13);
gen_test!(generated_variant_14, 14);
gen_test!(generated_variant_15, 15);
gen_test!(generated_variant_16, 16);
gen_test!(generated_variant_17, 17);
gen_test!(generated_variant_18, 18);
gen_test!(generated_variant_19, 19);
gen_test!(generated_variant_20, 20);
gen_test!(generated_variant_21, 21);
gen_test!(generated_variant_22, 22);
gen_test!(generated_variant_23, 23);
gen_test!(generated_variant_24, 24);
gen_test!(generated_variant_25, 25);
gen_test!(generated_variant_26, 26);
gen_test!(generated_variant_27, 27);
gen_test!(generated_variant_28, 28);
gen_test!(generated_variant_29, 29);
gen_test!(generated_variant_30, 30);
gen_test!(generated_variant_31, 31);
gen_test!(generated_variant_32, 32);
gen_test!(generated_variant_33, 33);
gen_test!(generated_variant_34, 34);
gen_test!(generated_variant_35, 35);
gen_test!(generated_variant_36, 36);
gen_test!(generated_variant_37, 37);
gen_test!(generated_variant_38, 38);
gen_test!(generated_variant_39, 39);
gen_test!(generated_variant_40, 40);
gen_test!(generated_variant_41, 41);
gen_test!(generated_variant_42, 42);
gen_test!(generated_variant_43, 43);
gen_test!(generated_variant_44, 44);
gen_test!(generated_variant_45, 45);
gen_test!(generated_variant_46, 46);
gen_test!(generated_variant_47, 47);
gen_test!(generated_variant_48, 48);
gen_test!(generated_variant_49, 49);

fn assert_all_products_found(
    idx: usize,
    det: &Detection,
    products: &[price_hunter_core::detect::Product],
) {
    for exp in products {
        assert!(
            det.products.iter().any(|p| p.name == exp.name && (p.price - exp.price).abs() < 0.01),
            "loop idx {} {} missing {:?}",
            idx,
            template_name(idx),
            exp
        );
    }
}

fn assert_template_roundtrip(
    idx: usize,
    products: &[price_hunter_core::detect::Product],
) {
    let html = render_template_by_idx(idx, products);
    // parse check
    let tree = Html::parse_document(&html);
    assert!(
        tree.tree.nodes().count() > 0,
        "loop idx {} {} unparsable",
        idx,
        template_name(idx)
    );
    let det = detect_grid(&html).unwrap_or_else(|| {
        let diag = diagnose_containers(&html);
        dump_and_panic(idx, &html, &diag, "detect_grid returned None (loop)")
    });
    assert_eq!(
        det.products.len(),
        products.len(),
        "loop idx {} {} expected {} got {}",
        idx,
        template_name(idx),
        products.len(),
        det.products.len()
    );
    assert_all_products_found(idx, &det, products);
}

#[test]
fn generated_loop_all_templates() {
    let products = fixed_products();
    for idx in 0..50 {
        assert_template_roundtrip(idx, &products);
    }
}
