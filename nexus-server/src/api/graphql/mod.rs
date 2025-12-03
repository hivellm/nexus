//! GraphQL API module
//!
//! This module provides a GraphQL API for querying and mutating graph data.
//! It automatically generates a GraphQL schema from the database catalog and
//! translates GraphQL queries to Cypher queries for execution.

mod mutation;
mod resolver;
mod schema;
mod types;

pub use mutation::*;
pub use resolver::*;
pub use schema::*;
pub use types::*;

use crate::NexusServer;
use async_graphql::{EmptySubscription, Schema};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{extract::State, response::IntoResponse};
use std::sync::Arc;

/// GraphQL schema type
pub type GraphQLSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

/// Create a new GraphQL schema from the Nexus server
pub fn create_schema(server: Arc<NexusServer>) -> GraphQLSchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(server)
        .finish()
}

/// GraphQL query handler
pub async fn graphql_handler(
    State(schema): State<GraphQLSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

/// GraphQL playground handler (for development)
#[cfg(debug_assertions)]
pub async fn graphql_playground() -> impl IntoResponse {
    use axum::response::Html;
    Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new("/graphql"),
    ))
}
