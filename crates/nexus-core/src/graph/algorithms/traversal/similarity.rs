//! Similarity metrics and clustering-coefficient algorithms.

use super::super::*;

impl Graph {
    /// Count triangles in the graph
    ///
    /// A triangle is a set of three nodes where each pair is connected by an edge.
    /// This algorithm counts the total number of triangles in the graph.
    ///
    /// # Returns
    ///
    /// The total number of triangles in the graph
    ///
    /// # Algorithm
    ///
    /// For each edge (u, v), find common neighbors of u and v.
    /// Each common neighbor w forms a triangle (u, v, w).
    /// To avoid counting each triangle three times (once for each edge),
    /// we only count when u < v < w in node ID ordering.
    pub fn triangle_count(&self) -> usize {
        let mut count = 0;

        // Convert adjacency list to HashSet for faster lookups
        let adjacency_sets: HashMap<u64, HashSet<u64>> = self
            .adjacency
            .iter()
            .map(|(&node, neighbors)| {
                let neighbor_set: HashSet<u64> = neighbors.iter().map(|(n, _)| *n).collect();
                (node, neighbor_set)
            })
            .collect();

        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();

        // For directed graphs, find cycles of length 3: u→v, v→w, w→u
        for &u in &nodes {
            if let Some(u_neighbors) = adjacency_sets.get(&u) {
                for &v in u_neighbors {
                    if let Some(v_neighbors) = adjacency_sets.get(&v) {
                        for &w in v_neighbors {
                            // Check if there's an edge w→u to complete the triangle
                            if let Some(w_neighbors) = adjacency_sets.get(&w) {
                                if w_neighbors.contains(&u) {
                                    // Found a triangle u→v→w→u
                                    // Only count each triangle once (when u < v < w)
                                    if u < v && v < w {
                                        count += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        count
    }

    /// Compute local clustering coefficient for each node
    ///
    /// The local clustering coefficient of a node is the ratio of triangles
    /// connected to the node to the maximum possible number of triangles
    /// (i.e., the number of pairs of neighbors).
    ///
    /// Formula: C(v) = 2 * T(v) / (deg(v) * (deg(v) - 1))
    /// where T(v) is the number of triangles through node v
    ///
    /// # Returns
    ///
    /// A HashMap mapping each node ID to its local clustering coefficient (0.0 to 1.0)
    pub fn clustering_coefficient(&self) -> HashMap<u64, f64> {
        let mut coefficients = HashMap::new();

        // Build undirected adjacency (treat graph as undirected)
        let mut undirected_adjacency: HashMap<u64, HashSet<u64>> = HashMap::new();

        for (&from, neighbors) in &self.adjacency {
            for &(to, _) in neighbors {
                undirected_adjacency
                    .entry(from)
                    .or_insert_with(HashSet::new)
                    .insert(to);
                undirected_adjacency
                    .entry(to)
                    .or_insert_with(HashSet::new)
                    .insert(from);
            }
        }

        // Ensure all nodes are in the map
        for node in self.adjacency.keys() {
            undirected_adjacency
                .entry(*node)
                .or_insert_with(HashSet::new);
        }

        for (&node, neighbors_set) in &undirected_adjacency {
            let degree = neighbors_set.len();

            if degree < 2 {
                // Need at least 2 neighbors to form a triangle
                coefficients.insert(node, 0.0);
                continue;
            }

            // Count triangles through this node
            let mut triangle_count = 0;
            let neighbors: Vec<u64> = neighbors_set.iter().cloned().collect();

            for i in 0..neighbors.len() {
                for j in (i + 1)..neighbors.len() {
                    let n1 = neighbors[i];
                    let n2 = neighbors[j];

                    // Check if n1 and n2 are connected (in undirected sense)
                    if let Some(n1_neighbors) = undirected_adjacency.get(&n1) {
                        if n1_neighbors.contains(&n2) {
                            triangle_count += 1;
                        }
                    }
                }
            }

            // Calculate coefficient: 2 * triangles / (deg * (deg - 1))
            let max_triangles = degree * (degree - 1);
            let coefficient = if max_triangles > 0 {
                (2.0 * triangle_count as f64) / max_triangles as f64
            } else {
                0.0
            };

            coefficients.insert(node, coefficient);
        }

        coefficients
    }

    /// Compute global clustering coefficient (transitivity)
    ///
    /// The global clustering coefficient is the ratio of the number of closed
    /// triplets (triangles) to the total number of triplets (connected triples).
    ///
    /// Formula: C = 3 * number_of_triangles / number_of_triplets
    ///
    /// # Returns
    ///
    /// The global clustering coefficient (0.0 to 1.0)
    pub fn global_clustering_coefficient(&self) -> f64 {
        // Build undirected adjacency (treat graph as undirected)
        let mut undirected_adjacency: HashMap<u64, HashSet<u64>> = HashMap::new();

        for (&from, neighbors) in &self.adjacency {
            for &(to, _) in neighbors {
                undirected_adjacency
                    .entry(from)
                    .or_insert_with(HashSet::new)
                    .insert(to);
                undirected_adjacency
                    .entry(to)
                    .or_insert_with(HashSet::new)
                    .insert(from);
            }
        }

        // Count undirected triangles
        let mut triangle_count = 0;
        let mut nodes: Vec<u64> = undirected_adjacency.keys().cloned().collect();
        nodes.sort(); // Ensure deterministic ordering

        for &u in &nodes {
            if let Some(u_neighbors) = undirected_adjacency.get(&u) {
                // Sort neighbors for deterministic ordering
                let mut neighbors: Vec<u64> = u_neighbors.iter().cloned().collect();
                neighbors.sort();
                for i in 0..neighbors.len() {
                    for j in (i + 1)..neighbors.len() {
                        let v = neighbors[i];
                        let w = neighbors[j];
                        if let Some(v_neighbors) = undirected_adjacency.get(&v) {
                            if v_neighbors.contains(&w) {
                                // Only count when u < v < w to avoid duplicates
                                if u < v && v < w {
                                    triangle_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        let triangles = triangle_count as f64 * 3.0; // Each triangle has 3 triplets

        // Count triplets (connected triples / wedges)
        let mut triplets = 0;
        for neighbors_set in undirected_adjacency.values() {
            let degree = neighbors_set.len();
            if degree >= 2 {
                // Number of ways to choose 2 neighbors: C(degree, 2) = degree*(degree-1)/2
                triplets += degree * (degree - 1) / 2;
            }
        }

        if triplets == 0 {
            0.0
        } else {
            triangles / triplets as f64
        }
    }

    /// Build packed u64 bitmaps of the neighbour sets of `node1` and
    /// `node2` over a common universe of bit positions (one per node in
    /// `self.adjacency`). The two bitmaps have identical length so the
    /// SIMD bitmap kernels can consume them directly.
    fn pack_neighbor_bitmaps(&self, node1: u64, node2: u64) -> Option<(Vec<u64>, Vec<u64>)> {
        if !self.has_node(node1) || !self.has_node(node2) {
            return None;
        }

        let mut idx_of: HashMap<u64, usize> = HashMap::with_capacity(self.adjacency.len());
        for (i, &n) in self.adjacency.keys().enumerate() {
            idx_of.insert(n, i);
        }
        let total = idx_of.len();
        let words = total.div_ceil(64);

        let mut bitmap1 = vec![0u64; words];
        let mut bitmap2 = vec![0u64; words];

        for (n, _) in self.get_neighbors(node1) {
            if let Some(&idx) = idx_of.get(n) {
                bitmap1[idx / 64] |= 1u64 << (idx % 64);
            }
        }
        for (n, _) in self.get_neighbors(node2) {
            if let Some(&idx) = idx_of.get(n) {
                bitmap2[idx / 64] |= 1u64 << (idx % 64);
            }
        }

        Some((bitmap1, bitmap2))
    }

    /// Calculate Jaccard Similarity between two nodes based on their
    /// neighbour sets. Returns `|A ∩ B| / |A ∪ B|` in the range
    /// `[0.0, 1.0]`, or `0.0` if either node is missing or both
    /// neighbour sets are empty.
    ///
    /// Uses the SIMD-dispatched `simd::bitmap::{popcount,and_popcount}`
    /// kernels — at 10K+ scale the bitmap AND+popcount path wins
    /// decisively over the previous `HashSet::intersection` on dense
    /// adjacency.
    pub fn jaccard_similarity(&self, node1: u64, node2: u64) -> f64 {
        let Some((b1, b2)) = self.pack_neighbor_bitmaps(node1, node2) else {
            return 0.0;
        };
        let intersection = crate::simd::bitmap::and_popcount_u64(&b1, &b2);
        let card1 = crate::simd::bitmap::popcount_u64(&b1);
        let card2 = crate::simd::bitmap::popcount_u64(&b2);
        let union = card1 + card2 - intersection;
        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }

    /// Calculate Cosine Similarity between two nodes (based on their
    /// binary-indicator neighbour vectors). Returns `|A ∩ B| / √(|A| *
    /// |B|)` in the range `[0.0, 1.0]` — negative values do not occur
    /// with 0/1 membership.
    ///
    /// SIMD-accelerated via `simd::bitmap::{popcount,and_popcount}`.
    pub fn cosine_similarity(&self, node1: u64, node2: u64) -> f64 {
        let Some((b1, b2)) = self.pack_neighbor_bitmaps(node1, node2) else {
            return 0.0;
        };
        let intersection = crate::simd::bitmap::and_popcount_u64(&b1, &b2) as f64;
        let card1 = crate::simd::bitmap::popcount_u64(&b1) as f64;
        let card2 = crate::simd::bitmap::popcount_u64(&b2) as f64;
        let denom = (card1 * card2).sqrt();
        if denom == 0.0 {
            0.0
        } else {
            intersection / denom
        }
    }

    /// Calculate Node Similarity for all pairs
    /// Returns a map of (node1, node2) -> similarity score
    pub fn node_similarity(&self, similarity_type: &str) -> HashMap<(u64, u64), f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let mut similarities = HashMap::new();

        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let node1 = nodes[i];
                let node2 = nodes[j];
                let similarity = match similarity_type {
                    "jaccard" => self.jaccard_similarity(node1, node2),
                    "cosine" => self.cosine_similarity(node1, node2),
                    _ => 0.0,
                };
                similarities.insert((node1, node2), similarity);
                similarities.insert((node2, node1), similarity); // Symmetric
            }
        }

        similarities
    }
}
