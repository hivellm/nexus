//! Cypher planning entrypoints: parse-and-plan, plan-a-pre-parsed-AST
//! (the cluster-mode preparsed-AST handoff lands here too), and the
//! legacy AST-to-operators lowering used outside the main planner.

use super::super::*;
use crate::Result;
use planner::QueryPlanner;

impl Executor {
    /// Parse Cypher into physical plan
    pub fn parse_and_plan(&self, cypher: &str) -> Result<Vec<Operator>> {
        // Use the parser to parse the query
        let mut parser = parser::CypherParser::new(cypher.to_string());
        let ast = parser.parse()?;
        // Semantic validation, on the AST this call just parsed — so it costs no
        // extra parse. This is the choke point for every query that reaches the
        // executor WITHOUT passing through `Engine`: both transports carve a pure
        // autocommit read out of the engine lock and run it straight on a cloned
        // `Executor` (`api::cypher::execute::handler` and the RPC `CYPHER`
        // dispatcher). Without a check here those reads were unvalidated on BOTH
        // transports, so which errors a client saw depended on whether its query
        // happened to need engine interception.
        //
        // Deliberately not in `plan_ast`: the cluster-mode handoff lands there
        // with an AST the engine already validated and then rewrote for tenant
        // scoping, and re-validating a rewritten AST buys nothing.
        crate::executor::semantic_validation::validate(&ast)?;
        self.plan_ast(&ast)
    }

    /// Plan a pre-parsed Cypher AST into physical operators.
    ///
    /// Shared tail of both [`Self::parse_and_plan`] (which parses
    /// from a string) and the cluster-mode `preparsed_ast` handoff
    /// (which lands here with an AST already rewritten for tenant
    /// scoping). Both paths must produce identical plans for the
    /// same AST — any divergence would be a cluster-mode
    /// correctness bug, so there is no second code path that does
    /// anything else.
    pub fn plan_ast(&self, ast: &parser::CypherQuery) -> Result<Vec<Operator>> {
        // Clone index data instead of holding locks during planning.
        // This reduces lock contention and allows better parallelization.
        let label_index_snapshot = {
            let _guard = self.label_index();
            _guard.clone()
        };
        let knn_index_snapshot = {
            let _guard = self.knn_index();
            _guard.clone()
        };

        // Locks are released here - planning happens with cloned data.
        // The property index handle is now threaded through ExecutorShared
        // (populated by `Engine::refresh_executor` via
        // `install_property_index`). Executors built outside an engine —
        // e.g. the test harness — still have `None` and accept
        // `USING INDEX` hints silently; that is intentional.
        let mut planner =
            QueryPlanner::new(self.catalog(), &label_index_snapshot, &knn_index_snapshot)
                .with_rtree(self.shared.rtree_registry.clone());
        if let Some(pi) = self.property_index() {
            planner = planner.with_property_index(pi);
        }
        if let Some(ci) = self.composite_btree() {
            planner = planner.with_composite_index(ci);
        }

        let mut operators = planner.plan_query(ast)?;

        // Optimize the operator order
        operators = planner.optimize_operator_order(operators)?;

        // Bridge planner-level diagnostics across the planner-drop
        // boundary so `Executor::execute` can attach them to the
        // resulting `ResultSet`. Empty vec is a no-op fast path.
        planner::queries::stash_planner_notifications(planner.take_notifications());

        Ok(operators)
    }

    /// Convert AST to physical operators
    pub(in super::super) fn ast_to_operators(
        &mut self,
        ast: &parser::CypherQuery,
    ) -> Result<Vec<Operator>> {
        let mut operators = Vec::new();

        for clause in &ast.clauses {
            match clause {
                parser::Clause::Match(match_clause) => {
                    // Add NodeByLabel operators for each node pattern
                    for element in &match_clause.pattern.elements {
                        if let parser::PatternElement::Node(node) = element {
                            if let Some(variable) = &node.variable {
                                if let Some(label) = node.labels.first() {
                                    let label_id = self.catalog().get_or_create_label(label)?;
                                    operators.push(Operator::NodeByLabel {
                                        label_id,
                                        variable: variable.clone(),
                                    });
                                }
                            }
                        }
                    }

                    // Add WHERE clause as Filter operator
                    if let Some(where_clause) = &match_clause.where_clause {
                        operators.push(Operator::Filter {
                            predicate: self.expression_to_string(&where_clause.expression)?,
                            predicate_ast: Some(Box::new(where_clause.expression.clone())),
                        });
                    }
                }
                parser::Clause::Create(create_clause) => {
                    // CREATE: create nodes and relationships from pattern
                    // Add CREATE operator (don't execute directly)
                    operators.push(Operator::Create {
                        pattern: create_clause.pattern.clone(),
                        external_id_expr: create_clause.external_id_expr.clone(),
                        conflict_policy: create_clause.conflict_policy,
                    });
                }
                parser::Clause::Merge(merge_clause) => {
                    // MERGE: match-or-create pattern
                    // For now, treat as MATCH - executor will handle match-or-create logic
                    for element in &merge_clause.pattern.elements {
                        if let parser::PatternElement::Node(node) = element {
                            if let Some(variable) = &node.variable {
                                if let Some(label) = node.labels.first() {
                                    let label_id = self.catalog().get_or_create_label(label)?;
                                    operators.push(Operator::NodeByLabel {
                                        label_id,
                                        variable: variable.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
                parser::Clause::Where(where_clause) => {
                    operators.push(Operator::Filter {
                        predicate: self.expression_to_string(&where_clause.expression)?,
                        predicate_ast: Some(Box::new(where_clause.expression.clone())),
                    });
                }
                parser::Clause::Return(return_clause) => {
                    let projection_items: Vec<ProjectionItem> = return_clause
                        .items
                        .iter()
                        .map(|item| ProjectionItem {
                            expression: item.expression.clone(),
                            alias: item.alias.clone().unwrap_or_else(|| {
                                self.expression_to_string(&item.expression)
                                    .unwrap_or_default()
                            }),
                        })
                        .collect();

                    operators.push(Operator::Project {
                        items: projection_items,
                    });
                }
                parser::Clause::Limit(limit_clause) => {
                    if let parser::Expression::Literal(parser::Literal::Integer(count)) =
                        &limit_clause.count
                    {
                        operators.push(Operator::Limit {
                            count: *count as usize,
                        });
                    }
                }
                _ => {
                    // Other clauses not implemented in MVP
                }
            }
        }

        Ok(operators)
    }
}
