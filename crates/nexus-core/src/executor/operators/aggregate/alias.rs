//! Alias resolution for aggregation result labelling.

use super::super::super::engine::Executor;
use super::super::super::types::Aggregation;

impl Executor {
    pub(in crate::executor) fn aggregation_alias(&self, aggregation: &Aggregation) -> String {
        match aggregation {
            Aggregation::Count { alias, .. }
            | Aggregation::Sum { alias, .. }
            | Aggregation::Avg { alias, .. }
            | Aggregation::Min { alias, .. }
            | Aggregation::Max { alias, .. }
            | Aggregation::Collect { alias, .. }
            | Aggregation::PercentileDisc { alias, .. }
            | Aggregation::PercentileCont { alias, .. }
            | Aggregation::StDev { alias, .. }
            | Aggregation::StDevP { alias, .. }
            | Aggregation::CountStarOptimized { alias, .. } => alias.clone(),
        }
    }
}
