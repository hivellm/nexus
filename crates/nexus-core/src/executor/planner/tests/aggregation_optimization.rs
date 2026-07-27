use super::*;

/// Check if aggregation can be optimized with streaming
pub fn can_use_streaming_aggregation(operators: &[Operator]) -> bool {
    // Check if we have aggregation operations that can benefit from streaming
    for operator in operators {
        if let Operator::Aggregate {
            group_by,
            aggregations,
            ..
        } = operator
        {
            // Streaming is beneficial when:
            // 1. We have aggregations that can be computed incrementally
            // 2. Group-by keys are not too numerous (to avoid memory explosion)
            // 3. We don't have complex expressions in aggregations

            if aggregations.len() > 10 {
                return false; // Too many aggregations, stick with in-memory
            }

            // Check aggregation types - streaming works best with COUNT, SUM, AVG
            for agg in aggregations {
                match agg {
                    Aggregation::Count { .. }
                    | Aggregation::Sum { .. }
                    | Aggregation::Avg { .. } => {
                        // These can be streamed
                    }
                    Aggregation::Min { .. } | Aggregation::Max { .. } => {
                        // These can also be streamed
                    }
                    Aggregation::Collect { .. } => {
                        // Collect requires storing all values, not suitable for streaming
                        return false;
                    }
                    Aggregation::CountStarOptimized { .. } => {
                        // Optimized count is already efficient
                    }
                    _ => {
                        // Other aggregations may not be suitable for streaming
                        return false;
                    }
                }
            }

            // Check group-by complexity
            if group_by.len() > 3 {
                return false; // Too many group-by keys for streaming
            }

            return true;
        }
    }
    false
}

/// Optimize aggregation operations by pushing them down in the query plan
pub fn optimize_aggregations(operators: Vec<Operator>) -> Result<Vec<Operator>> {
    let mut result = Vec::new();

    for operator in operators {
        match operator {
            Operator::Aggregate {
                ref aggregations,
                ref group_by,
                ref source,
                ..
            } => {
                // Check if we can push aggregation down to reduce data volume earlier
                if let Some(source_op) = source.as_ref() {
                    // Convert group_by from Vec<String> to Vec<Expression> for the check
                    // For now, we'll just check if we can push down (simplified)
                    let can_push = match source_op.as_ref() {
                        Operator::Filter { .. } | Operator::Project { .. } => true,
                        _ => false,
                    };
                    if can_push {
                        // Create a new aggregation operator with push-down optimization
                        let optimized_agg = Operator::Aggregate {
                            aggregations: aggregations.clone(),
                            group_by: group_by.clone(),
                            projection_items: None,
                            output_order: None,
                            source: source.clone(),
                            streaming_optimized: false,
                            push_down_optimized: true,
                        };
                        result.push(optimized_agg);
                        continue;
                    }
                }

                // Use streaming aggregation if beneficial
                if can_use_streaming_aggregation(&[operator.clone()]) {
                    let streaming_agg = Operator::Aggregate {
                        aggregations: aggregations.clone(),
                        group_by: group_by.clone(),
                        projection_items: None,
                        output_order: None,
                        source: source.clone(),
                        streaming_optimized: true,
                        push_down_optimized: false,
                    };
                    result.push(streaming_agg);
                    continue;
                }

                // Default aggregation
                result.push(operator);
            }
            _ => result.push(operator),
        }
    }

    Ok(result)
}

/// Check if aggregation can be pushed down to reduce data processing
fn can_push_aggregation_down(
    source_op: &Operator,
    aggregations: &[Aggregation],
    group_by: &[Expression],
) -> bool {
    match source_op {
        Operator::Filter { .. } => {
            // We can push aggregation past filters
            // Filter doesn't have a source field, so we can push down
            return true;
        }
        Operator::Project { .. } => {
            // Check if projection includes all needed columns for aggregation
            // Project doesn't have a source field, so we can push down
            return true;
        }
        Operator::Expand { .. } => {
            // Relationship expansions can sometimes be optimized with aggregation
            // For now, be conservative and don't push down
            return false;
        }
        _ => {
            // Other operators - check if they produce data we need for aggregation
            return source_supports_aggregation(source_op, aggregations, group_by);
        }
    }
}

/// Check if a source operator supports aggregation optimization
fn source_supports_aggregation(
    source_op: &Operator,
    _aggregations: &[Aggregation],
    _group_by: &[Expression],
) -> bool {
    match source_op {
        Operator::NodeByLabel { .. }
        | Operator::AllNodesScan { .. }
        | Operator::IndexScan { .. } => {
            // These are good sources for aggregation - they produce nodes we can aggregate
            true
        }
        Operator::Expand { .. } => {
            // Relationship traversal results can be aggregated
            true
        }
        _ => false,
    }
}

/// Create optimized COUNT operations
pub fn optimize_count_operations(operators: Vec<Operator>) -> Result<Vec<Operator>> {
    let mut result = Vec::new();

    for operator in operators {
        match operator {
            Operator::Aggregate {
                aggregations,
                group_by,
                source,
                ..
            } => {
                let mut optimized_aggregations = Vec::new();

                for agg in aggregations {
                    match agg {
                        Aggregation::Count { column: None, .. } => {
                            // Optimize COUNT(*) operations
                            if can_optimize_count_star(&source) {
                                optimized_aggregations.push(Aggregation::CountStarOptimized {
                                    alias: "count".to_string(), // Default alias
                                });
                            } else {
                                optimized_aggregations.push(agg);
                            }
                        }
                        _ => optimized_aggregations.push(agg),
                    }
                }

                result.push(Operator::Aggregate {
                    aggregations: optimized_aggregations,
                    group_by,
                    projection_items: None,
                    output_order: None,
                    source,
                    streaming_optimized: false,
                    push_down_optimized: false,
                });
            }
            _ => result.push(operator),
        }
    }

    Ok(result)
}

/// Check if COUNT(*) can be optimized (e.g., using index statistics)
fn can_optimize_count_star(source: &Option<Box<Operator>>) -> bool {
    if let Some(source_op) = source {
        match source_op.as_ref() {
            Operator::NodeByLabel { label_id, .. } => {
                // We can potentially use label index statistics for COUNT(*)
                // This would require label index to track counts per label
                let _ = label_id; // We'll use this in the future
                false // For now, not implemented
            }
            Operator::AllNodesScan { .. } => {
                // For all nodes, we could potentially use total node count
                false // For now, not implemented
            }
            _ => false,
        }
    } else {
        false
    }
}
