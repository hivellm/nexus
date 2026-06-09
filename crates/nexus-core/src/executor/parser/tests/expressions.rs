//! Tests for expression parsing: literals, operators, CASE, IS NULL, etc.

use super::*;

#[test]
fn test_parse_parameter() {
    let mut parser =
        CypherParser::new("MATCH (n:Person) WHERE n.name = $name RETURN n".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[1] {
        Clause::Where(where_clause) => match &where_clause.expression {
            Expression::BinaryOp { right, .. } => match right.as_ref() {
                Expression::Parameter(name) => {
                    assert_eq!(name, "name");
                }
                _ => panic!("Expected parameter"),
            },
            _ => panic!("Expected binary operation"),
        },
        _ => panic!("Expected where clause"),
    }
}

#[test]
fn test_debug_binary_expression() {
    let mut parser = CypherParser::new("n.age < 18".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::BinaryOp { left, op, right } => {
            assert_eq!(op, BinaryOperator::LessThan);
            match *left {
                Expression::PropertyAccess { variable, property } => {
                    assert_eq!(variable, "n");
                    assert_eq!(property, "age");
                }
                _ => panic!("Expected property access"),
            }
            match *right {
                Expression::Literal(Literal::Integer(value)) => {
                    assert_eq!(value, 18);
                }
                _ => panic!("Expected integer literal"),
            }
        }
        _ => panic!("Expected binary operation"),
    }
}

#[test]
fn test_debug_case_expression() {
    let mut parser =
        CypherParser::new("CASE WHEN n.age < 18 THEN 'minor' ELSE 'adult' END".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::Case {
            when_clauses,
            else_clause,
            ..
        } => {
            assert_eq!(when_clauses.len(), 1);
            assert!(else_clause.is_some());
        }
        _ => panic!("Expected case expression"),
    }
}

#[test]
fn test_debug_when_keyword() {
    let mut parser = CypherParser::new("WHEN n.age < 18 THEN 'minor' ELSE 'adult' END".to_string());

    // Test parsing WHEN keyword
    assert!(parser.peek_keyword("WHEN"));
    parser.expect_keyword("WHEN").unwrap();

    // Debug: print remaining input after WHEN
    let remaining = &parser.input[parser.pos..];
    tracing::debug!("Remaining after WHEN: '{}'", remaining);

    // Test parsing the condition
    let condition = parser.parse_expression().unwrap();
    match condition {
        Expression::BinaryOp {
            left: _,
            op,
            right: _,
        } => {
            assert_eq!(op, BinaryOperator::LessThan);
        }
        _ => panic!("Expected binary operation"),
    }

    // Debug: print remaining input after condition
    let remaining = &parser.input[parser.pos..];
    tracing::debug!("Remaining after condition: '{}'", remaining);

    // Debug: test peek_keyword for THEN
    tracing::debug!("peek_keyword('THEN'): {}", parser.peek_keyword("THEN"));

    // Test parsing THEN keyword
    assert!(parser.peek_keyword("THEN"));
    parser.expect_keyword("THEN").unwrap();
}

#[test]
fn test_parse_case_expression() {
    // Test simple binary expression first
    let mut parser = CypherParser::new("n.age < 18".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::BinaryOp { left, op, right } => {
            assert_eq!(op, BinaryOperator::LessThan);
            match *left {
                Expression::PropertyAccess { variable, property } => {
                    assert_eq!(variable, "n");
                    assert_eq!(property, "age");
                }
                _ => panic!("Expected property access"),
            }
            match *right {
                Expression::Literal(Literal::Integer(value)) => {
                    assert_eq!(value, 18);
                }
                _ => panic!("Expected integer literal"),
            }
        }
        _ => panic!("Expected binary operation"),
    }

    // Test simple CASE expression
    let mut parser =
        CypherParser::new("CASE WHEN n.age < 18 THEN 'minor' ELSE 'adult' END".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::Case {
            when_clauses,
            else_clause,
            ..
        } => {
            assert_eq!(when_clauses.len(), 1);
            assert!(else_clause.is_some());
        }
        _ => panic!("Expected case expression"),
    }

    // Now test full query
    let mut parser = CypherParser::new(
        "MATCH (n:Person) RETURN CASE WHEN n.age < 18 THEN 'minor' ELSE 'adult' END AS category"
            .to_string(),
    );
    let query = parser.parse().unwrap();

    match &query.clauses[1] {
        Clause::Return(return_clause) => match &return_clause.items[0].expression {
            Expression::Case {
                when_clauses,
                else_clause,
                ..
            } => {
                assert_eq!(when_clauses.len(), 1);
                assert!(else_clause.is_some());
            }
            _ => panic!("Expected case expression"),
        },
        _ => panic!("Expected return clause"),
    }
}

#[test]
fn test_parse_error_reporting() {
    let mut parser = CypherParser::new("MATCH (n:Person RETURN n".to_string());
    let result = parser.parse();

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.to_string().contains("line"));
    assert!(error.to_string().contains("column"));
}

#[test]
fn test_parse_string_literals() {
    let mut parser = CypherParser::new("\"Hello World\"".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::Literal(Literal::String(value)) => {
            assert_eq!(value, "Hello World");
        }
        _ => panic!("Expected string literal"),
    }
}

#[test]
fn test_parse_string_literals_single_quotes() {
    let mut parser = CypherParser::new("'Hello World'".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::Literal(Literal::String(value)) => {
            assert_eq!(value, "Hello World");
        }
        _ => panic!("Expected string literal"),
    }
}

#[test]
fn test_parse_string_escapes() {
    let mut parser = CypherParser::new("\"Hello\\nWorld\\tTest\"".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::Literal(Literal::String(value)) => {
            assert_eq!(value, "Hello\nWorld\tTest");
        }
        _ => panic!("Expected string literal"),
    }
}

#[test]
fn test_parse_float_literals() {
    let mut parser = CypherParser::new("3.141592653589793".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::Literal(Literal::Float(value)) => {
            assert!((value - std::f64::consts::PI).abs() < 1e-6);
        }
        _ => panic!("Expected float literal"),
    }
}

#[test]
fn test_parse_boolean_literals() {
    // Test true
    let mut parser = CypherParser::new("true".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::Literal(Literal::Boolean(value)) => {
            assert!(value);
        }
        _ => panic!("Expected boolean literal"),
    }

    // Test false
    let mut parser = CypherParser::new("false".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::Literal(Literal::Boolean(value)) => {
            assert!(!value);
        }
        _ => panic!("Expected boolean literal"),
    }
}

#[test]
fn test_parse_null_literal() {
    let mut parser = CypherParser::new("null".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::Literal(Literal::Null) => {}
        _ => panic!("Expected null literal"),
    }
}

#[test]
fn test_parse_list_expression() {
    let mut parser = CypherParser::new("[1, 2, 3, 'hello']".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::List(elements) => {
            assert_eq!(elements.len(), 4);
        }
        _ => panic!("Expected list expression"),
    }
}

#[test]
fn test_parse_map_expression() {
    let mut parser = CypherParser::new("{name: 'John', age: 30}".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::Map(properties) => {
            assert_eq!(properties.len(), 2);
            assert!(properties.contains_key("name"));
            assert!(properties.contains_key("age"));
        }
        _ => panic!("Expected map expression"),
    }
}

#[test]
fn test_parse_function_call() {
    let mut parser = CypherParser::new("count(n)".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::FunctionCall { name, args } => {
            assert_eq!(name, "count");
            assert_eq!(args.len(), 1);
        }
        _ => panic!("Expected function call"),
    }
}

#[test]
fn test_parse_binary_operators() {
    // Test addition
    let mut parser = CypherParser::new("a + b".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::BinaryOp { op, .. } => {
            assert_eq!(op, BinaryOperator::Add);
        }
        _ => panic!("Expected binary operation"),
    }

    // Test subtraction
    let mut parser = CypherParser::new("a - b".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::BinaryOp { op, .. } => {
            assert_eq!(op, BinaryOperator::Subtract);
        }
        _ => panic!("Expected binary operation"),
    }

    // Test multiplication
    let mut parser = CypherParser::new("a * b".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::BinaryOp { op, .. } => {
            assert_eq!(op, BinaryOperator::Multiply);
        }
        _ => panic!("Expected binary operation"),
    }

    // Test division
    let mut parser = CypherParser::new("a / b".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::BinaryOp { op, .. } => {
            assert_eq!(op, BinaryOperator::Divide);
        }
        _ => panic!("Expected binary operation"),
    }

    // Test equality
    let mut parser = CypherParser::new("a = b".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::BinaryOp { op, .. } => {
            assert_eq!(op, BinaryOperator::Equal);
        }
        _ => panic!("Expected binary operation"),
    }

    // Test inequality
    let mut parser = CypherParser::new("a != b".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::BinaryOp { op, .. } => {
            assert_eq!(op, BinaryOperator::NotEqual);
        }
        _ => panic!("Expected binary operation"),
    }

    // Test less than
    let mut parser = CypherParser::new("a < b".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::BinaryOp { op, .. } => {
            assert_eq!(op, BinaryOperator::LessThan);
        }
        _ => panic!("Expected binary operation"),
    }

    // Test less than or equal
    let mut parser = CypherParser::new("a <= b".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::BinaryOp { op, .. } => {
            assert_eq!(op, BinaryOperator::LessThanOrEqual);
        }
        _ => panic!("Expected binary operation"),
    }

    // Test greater than
    let mut parser = CypherParser::new("a > b".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::BinaryOp { op, .. } => {
            assert_eq!(op, BinaryOperator::GreaterThan);
        }
        _ => panic!("Expected binary operation"),
    }

    // Test greater than or equal
    let mut parser = CypherParser::new("a >= b".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::BinaryOp { op, .. } => {
            assert_eq!(op, BinaryOperator::GreaterThanOrEqual);
        }
        _ => panic!("Expected binary operation"),
    }

    // Test AND
    let mut parser = CypherParser::new("a AND b".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::BinaryOp { op, .. } => {
            assert_eq!(op, BinaryOperator::And);
        }
        _ => panic!("Expected binary operation"),
    }

    // Test OR
    let mut parser = CypherParser::new("a OR b".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::BinaryOp { op, .. } => {
            assert_eq!(op, BinaryOperator::Or);
        }
        _ => panic!("Expected binary operation"),
    }
}

#[test]
fn test_parse_unary_operators() {
    // Test unary minus
    let mut parser = CypherParser::new("-5".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::UnaryOp { op, .. } => {
            assert_eq!(op, UnaryOperator::Minus);
        }
        _ => panic!("Expected unary operation"),
    }

    // Test unary plus
    let mut parser = CypherParser::new("+5".to_string());
    let expr = parser.parse_expression().unwrap();
    match expr {
        Expression::UnaryOp { op, .. } => {
            assert_eq!(op, UnaryOperator::Plus);
        }
        _ => panic!("Expected unary operation"),
    }
}

#[test]
fn test_parse_parenthesized_expression() {
    let mut parser = CypherParser::new("(a + b) * c".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::BinaryOp { op, .. } => {
            assert_eq!(op, BinaryOperator::Multiply);
        }
        _ => panic!("Expected binary operation"),
    }
}

#[test]
fn test_parse_case_expression_with_input() {
    let mut parser = CypherParser::new(
        "CASE n.status WHEN 'active' THEN 'working' WHEN 'inactive' THEN 'idle' ELSE 'unknown' END"
            .to_string(),
    );
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::Case {
            input,
            when_clauses,
            else_clause,
        } => {
            assert!(input.is_some());
            assert_eq!(when_clauses.len(), 2);
            assert!(else_clause.is_some());
        }
        _ => panic!("Expected case expression"),
    }
}

#[test]
fn test_parse_case_expression_without_input() {
    let mut parser = CypherParser::new(
        "CASE WHEN n.age < 18 THEN 'minor' WHEN n.age < 65 THEN 'adult' ELSE 'senior' END"
            .to_string(),
    );
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::Case {
            input,
            when_clauses,
            else_clause,
        } => {
            assert!(input.is_none());
            assert_eq!(when_clauses.len(), 2);
            assert!(else_clause.is_some());
        }
        _ => panic!("Expected case expression"),
    }
}

#[test]
fn test_is_null_parsing() {
    let mut parser = CypherParser::new(
        "MATCH (n:Node) WHERE n.value IS NOT NULL RETURN count(*) AS count".to_string(),
    );
    let query = parser.parse().unwrap();

    assert_eq!(query.clauses.len(), 3); // MATCH, WHERE, RETURN

    // Check WHERE clause contains IsNull expression
    match &query.clauses[1] {
        Clause::Where(where_clause) => match &where_clause.expression {
            Expression::IsNull { expr, negated } => {
                assert!(*negated, "Should be IS NOT NULL");
                match &**expr {
                    Expression::PropertyAccess { variable, property } => {
                        assert_eq!(variable, "n");
                        assert_eq!(property, "value");
                    }
                    _ => panic!("Expected PropertyAccess in IsNull expression"),
                }
            }
            _ => panic!(
                "Expected IsNull expression in WHERE clause, got: {:?}",
                where_clause.expression
            ),
        },
        _ => panic!("Expected WHERE clause"),
    }
}

#[test]
fn test_is_null_simple() {
    let mut parser = CypherParser::new("MATCH (n) WHERE n.prop IS NULL RETURN n".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[1] {
        Clause::Where(where_clause) => match &where_clause.expression {
            Expression::IsNull { negated, .. } => {
                assert!(!*negated, "Should be IS NULL");
            }
            _ => panic!("Expected IsNull expression"),
        },
        _ => panic!("Expected WHERE clause"),
    }
}

#[test]
fn test_is_null_expression_only() {
    // Simulate what execute_filter does - parse just the expression
    let mut parser = CypherParser::new("n.value IS NOT NULL".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::IsNull {
            expr: inner,
            negated,
        } => {
            assert!(negated, "Should be IS NOT NULL");
            match *inner {
                Expression::PropertyAccess { variable, property } => {
                    assert_eq!(variable, "n");
                    assert_eq!(property, "value");
                }
                _ => panic!("Expected PropertyAccess"),
            }
        }
        _ => panic!("Expected IsNull expression, got: {:?}", expr),
    }
}

#[test]
fn test_is_null_expression_simple() {
    let mut parser = CypherParser::new("n.prop IS NULL".to_string());
    let expr = parser.parse_expression().unwrap();

    match expr {
        Expression::IsNull { negated, .. } => {
            assert!(!negated, "Should be IS NULL");
        }
        _ => panic!("Expected IsNull expression, got: {:?}", expr),
    }
}

#[test]
fn test_and_with_comparisons() {
    let mut parser = CypherParser::new("n.age >= 25 AND n.age <= 35".to_string());
    let expr = parser.parse_expression().unwrap();

    // Should be: BinaryOp(>=) AND BinaryOp(<=)
    match expr {
        Expression::BinaryOp { left, op, right } => {
            assert!(matches!(op, BinaryOperator::And), "Top level should be AND");

            // Left side: n.age >= 25
            match &*left {
                Expression::BinaryOp { op, .. } => {
                    assert!(
                        matches!(op, BinaryOperator::GreaterThanOrEqual),
                        "Left should be >="
                    );
                }
                _ => panic!("Left side should be BinaryOp, got: {:?}", left),
            }

            // Right side: n.age <= 35
            match &*right {
                Expression::BinaryOp { op, .. } => {
                    assert!(
                        matches!(op, BinaryOperator::LessThanOrEqual),
                        "Right should be <="
                    );
                }
                _ => panic!("Right side should be BinaryOp, got: {:?}", right),
            }
        }
        _ => panic!("Expected BinaryOp with AND, got: {:?}", expr),
    }
}
