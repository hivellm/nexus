//! Test Point literal in RETURN clause
use nexus_core::executor::Query;
use nexus_core::testing::create_test_executor;
use serde_json::Value;

#[test]
fn test_return_point_literal_2d() {
    let (mut executor, _ctx) = create_test_executor();

    let query = Query {
        cypher: "RETURN point({x: 1, y: 2, crs: 'cartesian'}) AS p".to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "p");

    // Check that the first value is a Point object
    if let Some(Value::Object(obj)) = result.rows[0].values.first() {
        assert!(obj.contains_key("x"), "Point should have 'x' field");
        assert!(obj.contains_key("y"), "Point should have 'y' field");
        assert!(obj.contains_key("crs"), "Point should have 'crs' field");

        if let Some(Value::Number(x)) = obj.get("x") {
            assert_eq!(x.as_f64().unwrap(), 1.0);
        } else {
            panic!("x should be a number");
        }

        if let Some(Value::Number(y)) = obj.get("y") {
            assert_eq!(y.as_f64().unwrap(), 2.0);
        } else {
            panic!("y should be a number");
        }

        if let Some(Value::String(crs)) = obj.get("crs") {
            assert_eq!(crs, "cartesian");
        } else {
            panic!("crs should be a string");
        }
    } else {
        panic!(
            "Expected Point object, got: {:?}",
            result.rows[0].values.first()
        );
    }
}
