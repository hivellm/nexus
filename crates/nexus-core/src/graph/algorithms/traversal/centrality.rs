//! Centrality algorithms: PageRank (sequential + parallel + weighted),
//! betweenness, closeness, degree, eigenvector.

use super::super::*;

impl Graph {
    /// Calculate PageRank for all nodes
    /// Returns a map of node_id -> PageRank score
    pub fn pagerank(
        &self,
        damping_factor: f64,
        max_iterations: usize,
        tolerance: f64,
    ) -> HashMap<u64, f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let n = nodes.len() as f64;
        let mut ranks = HashMap::new();

        // Initialize ranks
        for &node in &nodes {
            ranks.insert(node, 1.0 / n);
        }

        // Calculate out-degrees
        let mut out_degrees = HashMap::new();
        for (&node, neighbors) in &self.adjacency {
            out_degrees.insert(node, neighbors.len() as f64);
        }

        for _ in 0..max_iterations {
            let mut new_ranks = HashMap::new();
            let mut total_diff = 0.0;

            // Initialize new ranks
            for &node in &nodes {
                new_ranks.insert(node, (1.0 - damping_factor) / n);
            }

            // Distribute PageRank
            for (&node, neighbors) in &self.adjacency {
                let out_degree = out_degrees.get(&node).copied().unwrap_or(1.0);
                if out_degree > 0.0 {
                    let contribution = damping_factor * ranks[&node] / out_degree;
                    for &(neighbor, _) in neighbors {
                        *new_ranks.get_mut(&neighbor).unwrap() += contribution;
                    }
                } else {
                    // Dangling node - distribute evenly
                    let contribution = damping_factor * ranks[&node] / n;
                    for &target_node in &nodes {
                        *new_ranks.get_mut(&target_node).unwrap() += contribution;
                    }
                }
            }

            // Check convergence
            for &node in &nodes {
                total_diff += (new_ranks[&node] - ranks[&node]).abs();
            }

            ranks = new_ranks;
            if total_diff < tolerance {
                break;
            }
        }

        ranks
    }

    /// Calculate Weighted PageRank for all nodes
    /// Uses edge weights to determine contribution distribution
    /// Higher edge weights mean more PageRank flows through that edge
    /// Returns a map of node_id -> PageRank score
    pub fn weighted_pagerank(
        &self,
        damping_factor: f64,
        max_iterations: usize,
        tolerance: f64,
    ) -> HashMap<u64, f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let n = nodes.len() as f64;
        let mut ranks = HashMap::new();

        // Initialize ranks
        for &node in &nodes {
            ranks.insert(node, 1.0 / n);
        }

        // Calculate weighted out-degrees (sum of edge weights)
        let mut weighted_out_degrees = HashMap::new();
        for (&node, neighbors) in &self.adjacency {
            let total_weight: f64 = neighbors.iter().map(|&(_, weight)| weight).sum();
            weighted_out_degrees.insert(node, total_weight);
        }

        for _ in 0..max_iterations {
            let mut new_ranks = HashMap::new();
            let mut total_diff = 0.0;

            // Initialize new ranks with teleport probability
            for &node in &nodes {
                new_ranks.insert(node, (1.0 - damping_factor) / n);
            }

            // Distribute PageRank based on edge weights
            for (&node, neighbors) in &self.adjacency {
                let total_weight = weighted_out_degrees.get(&node).copied().unwrap_or(0.0);
                if total_weight > 0.0 {
                    for &(neighbor, weight) in neighbors {
                        // Contribution proportional to edge weight
                        let contribution = damping_factor * ranks[&node] * weight / total_weight;
                        *new_ranks.get_mut(&neighbor).unwrap() += contribution;
                    }
                } else {
                    // Dangling node - distribute evenly
                    let contribution = damping_factor * ranks[&node] / n;
                    for &target_node in &nodes {
                        *new_ranks.get_mut(&target_node).unwrap() += contribution;
                    }
                }
            }

            // Check convergence
            for &node in &nodes {
                total_diff += (new_ranks[&node] - ranks[&node]).abs();
            }

            ranks = new_ranks;
            if total_diff < tolerance {
                break;
            }
        }

        ranks
    }

    /// Calculate PageRank using parallel processing for large graphs
    /// Automatically switches to parallel mode for graphs with >1000 nodes
    /// Returns a map of node_id -> PageRank score
    pub fn pagerank_parallel(
        &self,
        damping_factor: f64,
        max_iterations: usize,
        tolerance: f64,
    ) -> HashMap<u64, f64> {
        use rayon::prelude::*;

        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let n = nodes.len();

        // For small graphs, use sequential version
        if n < 1000 {
            return self.pagerank(damping_factor, max_iterations, tolerance);
        }

        let n_f64 = n as f64;
        let mut ranks: HashMap<u64, f64> = nodes.iter().map(|&node| (node, 1.0 / n_f64)).collect();

        // Pre-calculate out-degrees for efficiency
        let out_degrees: HashMap<u64, f64> = self
            .adjacency
            .iter()
            .map(|(&node, neighbors)| (node, neighbors.len() as f64))
            .collect();

        // Pre-calculate incoming edges for each node (reverse graph)
        let mut incoming: HashMap<u64, Vec<(u64, f64)>> = HashMap::new();
        for &node in &nodes {
            incoming.insert(node, Vec::new());
        }
        for (&src, neighbors) in &self.adjacency {
            let out_degree = out_degrees.get(&src).copied().unwrap_or(1.0);
            for &(dst, _) in neighbors {
                if let Some(inc) = incoming.get_mut(&dst) {
                    inc.push((src, out_degree));
                }
            }
        }

        // Find dangling nodes (nodes with no outgoing edges)
        let dangling_nodes: Vec<u64> = nodes
            .iter()
            .filter(|&&node| out_degrees.get(&node).copied().unwrap_or(0.0) == 0.0)
            .cloned()
            .collect();

        for _ in 0..max_iterations {
            // Calculate dangling node contribution
            let dangling_sum: f64 = dangling_nodes.iter().map(|node| ranks[node]).sum();
            let dangling_contribution = damping_factor * dangling_sum / n_f64;

            // Calculate new ranks in parallel
            let new_ranks: HashMap<u64, f64> = nodes
                .par_iter()
                .map(|&node| {
                    let base = (1.0 - damping_factor) / n_f64 + dangling_contribution;
                    let incoming_contribution: f64 = incoming
                        .get(&node)
                        .map(|inc| {
                            inc.iter()
                                .map(|(src, out_deg)| damping_factor * ranks[src] / out_deg)
                                .sum()
                        })
                        .unwrap_or(0.0);
                    (node, base + incoming_contribution)
                })
                .collect();

            // Check convergence
            let total_diff: f64 = nodes
                .par_iter()
                .map(|node| (new_ranks[node] - ranks[node]).abs())
                .sum();

            ranks = new_ranks;
            if total_diff < tolerance {
                break;
            }
        }

        ranks
    }

    /// Calculate Betweenness Centrality for all nodes
    /// Returns a map of node_id -> betweenness centrality score
    pub fn betweenness_centrality(&self) -> HashMap<u64, f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let mut centrality = HashMap::new();

        for &node in &nodes {
            centrality.insert(node, 0.0);
        }

        // For each node as source, calculate shortest paths
        for &source in &nodes {
            let result = self.dijkstra(source, None).unwrap();

            // Count shortest paths through each node
            for &target in &nodes {
                if source == target {
                    continue;
                }
                if let Some(path) = self.reconstruct_path(&result.parents, source, target) {
                    // Count intermediate nodes
                    for &intermediate in &path[1..path.len() - 1] {
                        *centrality.get_mut(&intermediate).unwrap() += 1.0;
                    }
                }
            }
        }

        // Normalize by number of pairs (excluding self)
        let n = nodes.len() as f64;
        let normalization = (n - 1.0) * (n - 2.0);
        if normalization > 0.0 {
            for value in centrality.values_mut() {
                *value /= normalization;
            }
        }

        centrality
    }

    /// Calculate Closeness Centrality for all nodes
    /// Returns a map of node_id -> closeness centrality score
    pub fn closeness_centrality(&self) -> HashMap<u64, f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let mut centrality = HashMap::new();

        for &node in &nodes {
            let result = self.dijkstra(node, None).unwrap();

            let mut total_distance = 0.0;
            let mut reachable_count = 0;

            for &other in &nodes {
                if node != other {
                    if let Some(&dist) = result.distances.get(&other) {
                        if dist != f64::INFINITY {
                            total_distance += dist;
                            reachable_count += 1;
                        }
                    }
                }
            }

            if reachable_count > 0 && total_distance > 0.0 {
                centrality.insert(node, reachable_count as f64 / total_distance);
            } else {
                centrality.insert(node, 0.0);
            }
        }

        centrality
    }

    /// Calculate Degree Centrality for all nodes
    /// Returns a map of node_id -> degree centrality score
    pub fn degree_centrality(&self) -> HashMap<u64, f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let n = nodes.len() as f64;
        let mut centrality = HashMap::new();

        for &node in &nodes {
            let degree = self.get_neighbors(node).len() as f64;
            // Normalize by maximum possible degree (n-1)
            centrality.insert(node, if n > 1.0 { degree / (n - 1.0) } else { 0.0 });
        }

        centrality
    }

    /// Compute eigenvector centrality using power iteration
    ///
    /// Eigenvector centrality assigns scores to nodes based on the principle that
    /// connections to high-scoring nodes contribute more to the score of the node in question.
    ///
    /// Uses power iteration method with the following parameters:
    /// - max_iterations: Maximum number of iterations (default: 100)
    /// - tolerance: Convergence threshold (default: 1e-6)
    ///
    /// Returns normalized centrality scores (sum of squares = 1)
    pub fn eigenvector_centrality(&self) -> HashMap<u64, f64> {
        self.eigenvector_centrality_with_params(100, 1e-6)
    }

    /// Compute eigenvector centrality with custom parameters
    pub fn eigenvector_centrality_with_params(
        &self,
        max_iterations: usize,
        tolerance: f64,
    ) -> HashMap<u64, f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();

        if nodes.is_empty() {
            return HashMap::new();
        }

        // Initialize all scores to 1.0
        let mut scores: HashMap<u64, f64> = nodes.iter().map(|&n| (n, 1.0)).collect();

        // Build reverse adjacency (incoming edges)
        let mut reverse_adjacency: HashMap<u64, Vec<u64>> = HashMap::new();
        for &node in &nodes {
            reverse_adjacency.insert(node, Vec::new());
        }
        for (&from, neighbors) in &self.adjacency {
            for &(to, _weight) in neighbors {
                reverse_adjacency.entry(to).or_default().push(from);
            }
        }

        // Power iteration
        for _iteration in 0..max_iterations {
            let mut new_scores = HashMap::new();

            // Update scores: score[node] = sum(score[neighbor]) for incoming neighbors
            for &node in &nodes {
                let incoming = reverse_adjacency.get(&node).unwrap();
                let score: f64 = incoming
                    .iter()
                    .map(|&n| scores.get(&n).unwrap_or(&0.0))
                    .sum();
                new_scores.insert(node, score);
            }

            // Normalize scores (L2 normalization)
            let norm: f64 = new_scores.values().map(|&s| s * s).sum::<f64>().sqrt();

            if norm == 0.0 {
                // Graph has no edges, return uniform distribution
                let uniform_score = 1.0 / (nodes.len() as f64).sqrt();
                return nodes.iter().map(|&n| (n, uniform_score)).collect();
            }

            for score in new_scores.values_mut() {
                *score /= norm;
            }

            // Check convergence
            let diff: f64 = nodes
                .iter()
                .map(|&n| {
                    let old = scores.get(&n).unwrap_or(&0.0);
                    let new = new_scores.get(&n).unwrap_or(&0.0);
                    (old - new).abs()
                })
                .sum();

            scores = new_scores;

            if diff < tolerance {
                break;
            }
        }

        scores
    }
}
