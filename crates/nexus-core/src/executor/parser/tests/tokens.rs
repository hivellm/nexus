//! Tests for low-level token/lexer helpers: operators, identifiers,
//! keywords, character lookahead, whitespace skipping, and error creation.

use super::*;

#[test]
fn test_parse_comparison_operators() {
    // Test parse_comparison_operator method
    let mut parser = CypherParser::new("=".to_string());
    let op = parser.parse_comparison_operator().unwrap();
    assert_eq!(op, BinaryOperator::Equal);

    let mut parser = CypherParser::new("!=".to_string());
    let op = parser.parse_comparison_operator().unwrap();
    assert_eq!(op, BinaryOperator::NotEqual);

    let mut parser = CypherParser::new("<".to_string());
    let op = parser.parse_comparison_operator().unwrap();
    assert_eq!(op, BinaryOperator::LessThan);

    let mut parser = CypherParser::new("<=".to_string());
    let op = parser.parse_comparison_operator().unwrap();
    assert_eq!(op, BinaryOperator::LessThanOrEqual);

    let mut parser = CypherParser::new(">".to_string());
    let op = parser.parse_comparison_operator().unwrap();
    assert_eq!(op, BinaryOperator::GreaterThan);

    let mut parser = CypherParser::new(">=".to_string());
    let op = parser.parse_comparison_operator().unwrap();
    assert_eq!(op, BinaryOperator::GreaterThanOrEqual);
}

#[test]
fn test_parse_additive_operators() {
    // Test parse_additive_operator method
    let mut parser = CypherParser::new("+".to_string());
    let op = parser.parse_additive_operator().unwrap();
    assert_eq!(op, BinaryOperator::Add);

    let mut parser = CypherParser::new("-".to_string());
    let op = parser.parse_additive_operator().unwrap();
    assert_eq!(op, BinaryOperator::Subtract);
}

#[test]
fn test_parse_multiplicative_operators() {
    // Test parse_multiplicative_operator method
    let mut parser = CypherParser::new("*".to_string());
    let op = parser.parse_multiplicative_operator().unwrap();
    assert_eq!(op, BinaryOperator::Multiply);

    let mut parser = CypherParser::new("/".to_string());
    let op = parser.parse_multiplicative_operator().unwrap();
    assert_eq!(op, BinaryOperator::Divide);

    let mut parser = CypherParser::new("%".to_string());
    let op = parser.parse_multiplicative_operator().unwrap();
    assert_eq!(op, BinaryOperator::Modulo);

    let mut parser = CypherParser::new("^".to_string());
    let op = parser.parse_multiplicative_operator().unwrap();
    assert_eq!(op, BinaryOperator::Power);
}

#[test]
fn test_parse_unary_operators_method() {
    // Test parse_unary_operator method
    let mut parser = CypherParser::new("+".to_string());
    let op = parser.parse_unary_operator().unwrap();
    assert_eq!(op, UnaryOperator::Plus);

    let mut parser = CypherParser::new("-".to_string());
    let op = parser.parse_unary_operator().unwrap();
    assert_eq!(op, UnaryOperator::Minus);
}

#[test]
fn test_parse_primary_expression() {
    // Test parse_primary_expression method
    let mut parser = CypherParser::new("(a + b)".to_string());
    let expr = parser.parse_primary_expression().unwrap();
    match expr {
        Expression::BinaryOp { .. } => {}
        _ => panic!("Expected binary operation"),
    }

    let mut parser = CypherParser::new("$param".to_string());
    let expr = parser.parse_primary_expression().unwrap();
    match expr {
        Expression::Parameter(name) => {
            assert_eq!(name, "param");
        }
        _ => panic!("Expected parameter"),
    }
}

#[test]
fn test_parse_range_quantifier_edge_cases() {
    // Test basic relationship parsing
    let mut parser = CypherParser::new("MATCH (a)-[r]->(b) RETURN a".to_string());
    let query = parser.parse().unwrap();
    match &query.clauses[0] {
        Clause::Match(match_clause) => {
            assert_eq!(match_clause.pattern.elements.len(), 3); // node, rel, node
        }
        _ => panic!("Expected match clause"),
    }
}

#[test]
fn test_parse_identifier_validation() {
    // Test is_identifier_start
    let parser = CypherParser::new("a".to_string());
    assert!(parser.is_identifier_start());

    let parser = CypherParser::new("_".to_string());
    assert!(parser.is_identifier_start());

    let parser = CypherParser::new("1".to_string());
    assert!(!parser.is_identifier_start());

    // Test is_identifier_char
    let parser = CypherParser::new("a1_".to_string());
    assert!(parser.is_identifier_char());

    let parser = CypherParser::new(" ".to_string());
    assert!(!parser.is_identifier_char());

    // Test is_digit
    let parser = CypherParser::new("5".to_string());
    assert!(parser.is_digit());

    let parser = CypherParser::new("a".to_string());
    assert!(!parser.is_digit());

    // Test is_keyword_char
    let parser = CypherParser::new("a".to_string());
    assert!(parser.is_keyword_char());

    let parser = CypherParser::new("_".to_string());
    assert!(parser.is_keyword_char());

    let parser = CypherParser::new("1".to_string());
    assert!(!parser.is_keyword_char());
}

#[test]
fn test_parse_clause_boundary() {
    // Test is_clause_boundary
    let parser = CypherParser::new("MATCH".to_string());
    assert!(parser.is_clause_boundary());

    let parser = CypherParser::new("WHERE".to_string());
    assert!(parser.is_clause_boundary());

    let parser = CypherParser::new("RETURN".to_string());
    assert!(parser.is_clause_boundary());

    let parser = CypherParser::new("ORDER".to_string());
    assert!(parser.is_clause_boundary());

    let parser = CypherParser::new("LIMIT".to_string());
    assert!(parser.is_clause_boundary());

    let parser = CypherParser::new("SKIP".to_string());
    assert!(parser.is_clause_boundary());

    let parser = CypherParser::new("SELECT".to_string());
    assert!(!parser.is_clause_boundary());
}

#[test]
fn test_parse_peek_keyword() {
    // Test peek_keyword
    let parser = CypherParser::new("MATCH (n) RETURN n".to_string());
    assert!(parser.peek_keyword("MATCH"));

    let parser = CypherParser::new("  MATCH (n) RETURN n".to_string());
    assert!(parser.peek_keyword("MATCH"));

    let parser = CypherParser::new("MATCHING (n) RETURN n".to_string());
    assert!(!parser.peek_keyword("MATCH"));

    let parser = CypherParser::new("match (n) RETURN n".to_string());
    assert!(parser.peek_keyword("MATCH"));
}

#[test]
fn test_parse_error_handling() {
    // Test error creation
    let parser = CypherParser::new("test".to_string());
    let error = parser.error("Test error");
    assert!(error.to_string().contains("Test error"));
    assert!(error.to_string().contains("line"));
    assert!(error.to_string().contains("column"));
}

#[test]
fn test_parse_consume_char() {
    let mut parser = CypherParser::new("abc".to_string());

    assert_eq!(parser.consume_char(), Some('a'));
    assert_eq!(parser.pos, 1);
    assert_eq!(parser.line, 1);
    assert_eq!(parser.column, 2);

    assert_eq!(parser.consume_char(), Some('b'));
    assert_eq!(parser.pos, 2);
    assert_eq!(parser.line, 1);
    assert_eq!(parser.column, 3);

    assert_eq!(parser.consume_char(), Some('c'));
    assert_eq!(parser.pos, 3);
    assert_eq!(parser.line, 1);
    assert_eq!(parser.column, 4);

    assert_eq!(parser.consume_char(), None);
}

#[test]
fn test_parse_consume_char_newline() {
    let mut parser = CypherParser::new("a\nb".to_string());

    assert_eq!(parser.consume_char(), Some('a'));
    assert_eq!(parser.line, 1);
    assert_eq!(parser.column, 2);

    assert_eq!(parser.consume_char(), Some('\n'));
    assert_eq!(parser.line, 2);
    assert_eq!(parser.column, 1);

    assert_eq!(parser.consume_char(), Some('b'));
    assert_eq!(parser.line, 2);
    assert_eq!(parser.column, 2);
}

#[test]
fn test_parse_expect_char() {
    let mut parser = CypherParser::new("abc".to_string());

    assert!(parser.expect_char('a').is_ok());
    assert!(parser.expect_char('b').is_ok());
    assert!(parser.expect_char('c').is_ok());
    assert!(parser.expect_char('d').is_err());
}

#[test]
fn test_parse_expect_keyword() {
    let mut parser = CypherParser::new("MATCH (n) RETURN n".to_string());

    assert!(parser.expect_keyword("MATCH").is_ok());
    assert!(parser.expect_keyword("WHERE").is_err());
}

#[test]
fn test_parse_skip_whitespace() {
    let mut parser = CypherParser::new("   \t\n  abc".to_string());

    parser.skip_whitespace();
    assert_eq!(parser.pos, 7); // Should skip all whitespace (3 spaces + tab + newline + 2 spaces)
    assert_eq!(parser.peek_char(), Some('a'));
}

#[test]
fn test_parse_peek_char() {
    let parser = CypherParser::new("abc".to_string());

    assert_eq!(parser.peek_char(), Some('a'));

    let parser = CypherParser::new("".to_string());
    assert_eq!(parser.peek_char(), None);
}

#[test]
fn test_parse_peek_char_at() {
    let parser = CypherParser::new("abc".to_string());

    assert_eq!(parser.peek_char_at(0), Some('a'));
    assert_eq!(parser.peek_char_at(1), Some('b'));
    assert_eq!(parser.peek_char_at(2), Some('c'));
    assert_eq!(parser.peek_char_at(3), None);
}

#[test]
fn test_parse_number() {
    let mut parser = CypherParser::new("123".to_string());
    let number = parser.parse_number().unwrap();
    assert_eq!(number, 123);

    let mut parser = CypherParser::new("abc".to_string());
    assert!(parser.parse_number().is_err());
}

#[test]
fn test_parse_identifier() {
    let mut parser = CypherParser::new("abc123".to_string());
    let identifier = parser.parse_identifier().unwrap();
    assert_eq!(identifier, "abc123");

    let mut parser = CypherParser::new("_test".to_string());
    let identifier = parser.parse_identifier().unwrap();
    assert_eq!(identifier, "_test");

    let mut parser = CypherParser::new("123abc".to_string());
    assert!(parser.parse_identifier().is_err());
}

#[test]
fn test_parse_keyword() {
    let mut parser = CypherParser::new("MATCH".to_string());
    let keyword = parser.parse_keyword().unwrap();
    assert_eq!(keyword, "MATCH");

    let mut parser = CypherParser::new("  MATCH  ".to_string());
    let keyword = parser.parse_keyword().unwrap();
    assert_eq!(keyword, "MATCH");
}
