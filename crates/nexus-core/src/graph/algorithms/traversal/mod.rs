//! Traversal / pathfinding / community-detection algorithms for the
//! in-memory `Graph`. BFS, DFS, Dijkstra, A*, k-shortest-paths,
//! connected components (weak + strong), topological sort, minimum
//! spanning tree, clustering coefficients, betweenness centrality, etc.
//! Lives in its own directory module so each algorithm family is cohesive.
//!
//! All items previously reachable at `crate::graph::algorithms::traversal::<X>`
//! remain reachable at that path — this module is a transparent facade.

mod bfs_dfs;
mod centrality;
mod components;
mod mst;
mod shortest_path;
mod similarity;
