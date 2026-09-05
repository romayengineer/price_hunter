use crate::domain::detect::{Container, Detection, Product};

#[test]
fn sample_detection_builds_one_capture_and_two_products() {
    let detection = Detection {
        container: Container {
            classes: vec!["products".to_string(), "row".to_string()],
            id: Some("grid-1".to_string()),
            child_count: 2,
        },
        products: vec![
            Product {
                name: "Light Blue Homme EDP 50".to_string(),
                price_text: "242.100".to_string(),
                price: 242100.0,
                ..Product::default()
            },
            Product {
                name: "212 Vip EDP 80".to_string(),
                price_text: "278.100".to_string(),
                price: 278100.0,
                ..Product::default()
            },
        ],
    };
    assert_eq!(detection.products.len(), 2);
    assert_eq!(detection.container.child_count, 2);
}
