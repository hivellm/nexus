# Tasks - Advanced Features

## 1. FOREACH Clause ✅ COMPLETED
- [x] 1.1 Add ForeachClause to parser (ForeachClause struct and ForeachUpdateClause enum)
- [x] 1.2 Implement iteration over lists (execute_foreach_clause with variable and literal list support)
- [x] 1.3 Support SET/DELETE in FOREACH (SET and DELETE/DETACH DELETE operations)
- [x] 1.4 Add tests (6 comprehensive tests in foreach_test.rs)

## 2. EXISTS Subqueries ✅ COMPLETED
- [x] 2.1 Add EXISTS to WHERE parsing (Expression::Exists with Pattern)
- [x] 2.2 Implement existential pattern checks (check_pattern_exists function)
- [x] 2.3 Optimize with planner (basic implementation, can be enhanced later)
- [x] 2.4 Add tests (6 comprehensive tests in exists_test.rs)

## 3. CASE Expressions ✅ COMPLETED
- [x] 3.1 Add CaseExpression to AST (already existed)
- [x] 3.2 Implement simple CASE (evaluate_expression and evaluate_projection_expression)
- [x] 3.3 Implement generic CASE (with input expression comparison)
- [x] 3.4 Add tests (6 comprehensive tests in case_expression_test.rs)

## 4. Map Projections ✅ COMPLETED
- [x] 4.1 Add MapProjection to AST (Expression::MapProjection with MapProjectionItem enum)
- [x] 4.2 Implement property selection (.name, .age syntax)
- [x] 4.3 Support virtual keys (name: expression syntax) and aliases (.name AS alias)
- [x] 4.4 Add tests (6 comprehensive tests in map_projection_test.rs)

## 5. List Comprehensions ✅ COMPLETED
- [x] 5.1 Add ListComprehension to AST (Expression::ListComprehension with variable, list_expression, where_clause, transform_expression)
- [x] 5.2 Implement filtering (WHERE clause support in list comprehension evaluation)
- [x] 5.3 Implement transformation (transform expression support with | operator)
- [x] 5.4 Add tests (11 comprehensive tests in list_comprehension_test.rs)

## 6. Pattern Comprehensions ✅ COMPLETED
- [x] 6.1 Add PatternComprehension to AST (Expression::PatternComprehension with pattern, where_clause, transform_expression)
- [x] 6.2 Implement pattern collection (simplified implementation that works within current row context)
- [x] 6.3 Add tests (6 comprehensive tests in pattern_comprehension_test.rs)

## 7. Quality
- [ ] 7.1 95%+ coverage
- [ ] 7.2 No clippy warnings
- [ ] 7.3 Update documentation
