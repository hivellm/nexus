//! Shortest-path algorithms: Dijkstra, A*, Yen's k-shortest, Bellman-Ford.

use super::super::*;

impl Graph {
    /// Find shortest path using Dijkstra's algorithm
    pub fn dijkstra(&self, source: u64, target: Option<u64>) -> Result<ShortestPathResult> {
        if !self.has_node(source) {
            return Err(Error::InvalidInput("Source node not found".to_string()));
        }

        let mut distances = HashMap::new();
        let mut parents = HashMap::new();
        let mut heap = BinaryHeap::new();

        // Initialize distances
        for &node in self.adjacency.keys() {
            distances.insert(node, f64::INFINITY);
        }
        distances.insert(source, 0.0);

        // Priority queue item (negative distance for max-heap behavior)
        #[derive(PartialEq)]
        struct QueueItem(f64, u64);

        impl Eq for QueueItem {}

        impl PartialOrd for QueueItem {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for QueueItem {
            fn cmp(&self, other: &Self) -> Ordering {
                // Reverse the comparison for min-heap behavior
                other.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal)
            }
        }

        heap.push(QueueItem(0.0, source));

        while let Some(QueueItem(dist, current)) = heap.pop() {
            if dist > distances[&current] {
                continue;
            }

            for &(neighbor, weight) in self.get_neighbors(current) {
                let new_dist = dist + weight;
                if new_dist < distances[&neighbor] {
                    distances.insert(neighbor, new_dist);
                    parents.insert(neighbor, current);
                    heap.push(QueueItem(new_dist, neighbor));
                }
            }
        }

        // Reconstruct path if target is specified
        let path = if let Some(target) = target {
            self.reconstruct_path(&parents, source, target)
        } else {
            None
        };

        Ok(ShortestPathResult {
            distances,
            parents,
            path,
        })
    }

    /// Find shortest path using A* algorithm
    pub fn astar(
        &self,
        source: u64,
        target: u64,
        heuristic: impl Fn(u64, u64) -> f64,
    ) -> Result<ShortestPathResult> {
        if !self.has_node(source) || !self.has_node(target) {
            return Err(Error::InvalidInput(
                "Source or target node not found".to_string(),
            ));
        }

        let mut distances = HashMap::new();
        let mut parents = HashMap::new();
        let mut heap = BinaryHeap::new();

        // Initialize distances
        for &node in self.adjacency.keys() {
            distances.insert(node, f64::INFINITY);
        }
        distances.insert(source, 0.0);

        // Priority queue item (f_score, g_score, node_id)
        #[derive(PartialEq)]
        struct QueueItem(f64, f64, u64);

        impl Eq for QueueItem {}

        impl PartialOrd for QueueItem {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for QueueItem {
            fn cmp(&self, other: &Self) -> Ordering {
                self.partial_cmp(other).unwrap_or(Ordering::Equal)
            }
        }

        let f_score = 0.0 + heuristic(source, target);
        heap.push(QueueItem(f_score, 0.0, source));

        while let Some(QueueItem(_, g_score, current)) = heap.pop() {
            if current == target {
                break;
            }

            if g_score > distances[&current] {
                continue;
            }

            for &(neighbor, weight) in self.get_neighbors(current) {
                let tentative_g = g_score + weight;
                if tentative_g < distances[&neighbor] {
                    distances.insert(neighbor, tentative_g);
                    parents.insert(neighbor, current);
                    let f_score = tentative_g + heuristic(neighbor, target);
                    heap.push(QueueItem(f_score, tentative_g, neighbor));
                }
            }
        }

        let path = self.reconstruct_path(&parents, source, target);

        Ok(ShortestPathResult {
            distances,
            parents,
            path,
        })
    }

    /// Find K shortest paths using Yen's algorithm
    ///
    /// This algorithm finds the K shortest simple paths from source to target.
    /// Returns a vector of paths sorted by length (shortest first).
    ///
    /// # Arguments
    ///
    /// * `source` - The source node ID
    /// * `target` - The target node ID
    /// * `k` - Number of shortest paths to find
    ///
    /// # Returns
    ///
    /// A vector of `KShortestPath` containing up to K paths with their lengths
    pub fn k_shortest_paths(
        &self,
        source: u64,
        target: u64,
        k: usize,
    ) -> Result<Vec<KShortestPath>> {
        if !self.has_node(source) || !self.has_node(target) {
            return Err(Error::InvalidInput(
                "Source or target node not found".to_string(),
            ));
        }

        if k == 0 {
            return Ok(Vec::new());
        }

        // Find the first shortest path using Dijkstra
        let first_result = self.dijkstra(source, Some(target))?;
        if first_result.path.is_none() {
            return Ok(Vec::new()); // No path exists
        }

        let mut result_paths = Vec::new();
        let first_path = first_result.path.unwrap();
        let first_length = *first_result
            .distances
            .get(&target)
            .unwrap_or(&f64::INFINITY);

        result_paths.push(KShortestPath {
            path: first_path.clone(),
            length: first_length,
        });

        // Candidate paths (heap sorted by length)
        let mut candidates: Vec<KShortestPath> = Vec::new();

        for k_iter in 1..k {
            if result_paths.len() != k_iter {
                break; // No more paths found in previous iteration
            }

            let prev_path = &result_paths[k_iter - 1].path;

            // For each node in the previous shortest path
            for i in 0..(prev_path.len().saturating_sub(1)) {
                let spur_node = prev_path[i];
                let root_path = &prev_path[0..=i];

                // Build a modified graph by removing edges
                let mut removed_edges: Vec<(u64, u64)> = Vec::new();

                // Remove edges that are part of previous paths with the same root
                for prev_result_path in &result_paths {
                    let prev = &prev_result_path.path;
                    if prev.len() > i && &prev[0..=i] == root_path {
                        if i + 1 < prev.len() {
                            removed_edges.push((prev[i], prev[i + 1]));
                        }
                    }
                }

                // Create a temporary graph without the removed edges
                let spur_path = self.dijkstra_with_excluded_edges(
                    spur_node,
                    Some(target),
                    &removed_edges,
                    root_path,
                )?;

                if let Some(spur) = spur_path.path {
                    // Combine root path + spur path
                    let mut total_path = root_path[0..root_path.len() - 1].to_vec();
                    total_path.extend(spur);

                    // Calculate total length
                    let total_length = self.calculate_path_length(&total_path);

                    // Add to candidates if not already present
                    let candidate = KShortestPath {
                        path: total_path,
                        length: total_length,
                    };

                    if !candidates.iter().any(|c| c.path == candidate.path)
                        && !result_paths.iter().any(|r| r.path == candidate.path)
                    {
                        candidates.push(candidate);
                    }
                }
            }

            if candidates.is_empty() {
                break; // No more paths to find
            }

            // Sort candidates by length and take the shortest
            candidates.sort_by(|a, b| a.length.partial_cmp(&b.length).unwrap());
            result_paths.push(candidates.remove(0));
        }

        Ok(result_paths)
    }

    /// Helper function for Yen's algorithm - Dijkstra with excluded edges and nodes
    fn dijkstra_with_excluded_edges(
        &self,
        source: u64,
        target: Option<u64>,
        excluded_edges: &[(u64, u64)],
        excluded_nodes: &[u64],
    ) -> Result<ShortestPathResult> {
        let excluded_edges_set: HashSet<(u64, u64)> = excluded_edges.iter().cloned().collect();
        let excluded_nodes_set: HashSet<u64> = excluded_nodes[0..excluded_nodes.len() - 1]
            .iter()
            .cloned()
            .collect();

        let mut distances = HashMap::new();
        let mut parents = HashMap::new();
        let mut heap = BinaryHeap::new();

        // Initialize distances
        for &node in self.adjacency.keys() {
            if !excluded_nodes_set.contains(&node) {
                distances.insert(node, f64::INFINITY);
            }
        }
        distances.insert(source, 0.0);

        #[derive(PartialEq)]
        struct State {
            cost: f64,
            node: u64,
        }

        impl Eq for State {}

        impl Ord for State {
            fn cmp(&self, other: &Self) -> Ordering {
                other
                    .cost
                    .partial_cmp(&self.cost)
                    .unwrap_or(Ordering::Equal)
            }
        }

        impl PartialOrd for State {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        heap.push(State {
            cost: 0.0,
            node: source,
        });

        while let Some(State { cost, node }) = heap.pop() {
            if let Some(target_node) = target {
                if node == target_node {
                    break;
                }
            }

            if cost > *distances.get(&node).unwrap_or(&f64::INFINITY) {
                continue;
            }

            for &(neighbor, weight) in self.get_neighbors(node) {
                // Skip excluded edges and nodes
                if excluded_edges_set.contains(&(node, neighbor))
                    || excluded_nodes_set.contains(&neighbor)
                {
                    continue;
                }

                let new_cost = cost + weight;
                if new_cost < *distances.get(&neighbor).unwrap_or(&f64::INFINITY) {
                    distances.insert(neighbor, new_cost);
                    parents.insert(neighbor, node);
                    heap.push(State {
                        cost: new_cost,
                        node: neighbor,
                    });
                }
            }
        }

        let path = if let Some(target_node) = target {
            self.reconstruct_path(&parents, source, target_node)
        } else {
            None
        };

        Ok(ShortestPathResult {
            distances,
            parents,
            path,
        })
    }

    /// Helper function to calculate the total length of a path
    pub(in crate::graph::algorithms) fn calculate_path_length(&self, path: &[u64]) -> f64 {
        let mut length = 0.0;
        for i in 0..(path.len().saturating_sub(1)) {
            let from = path[i];
            let to = path[i + 1];
            if let Some(neighbors) = self.adjacency.get(&from) {
                if let Some((_, weight)) = neighbors.iter().find(|(n, _)| *n == to) {
                    length += weight;
                }
            }
        }
        length
    }

    /// Find shortest path using Bellman-Ford algorithm
    /// Can detect negative cycles
    pub fn bellman_ford(&self, source: u64) -> Result<(ShortestPathResult, bool)> {
        if !self.has_node(source) {
            return Err(Error::InvalidInput("Source node not found".to_string()));
        }

        let mut distances = HashMap::new();
        let mut parents = HashMap::new();

        // Initialize distances
        for &node in self.adjacency.keys() {
            distances.insert(node, f64::INFINITY);
        }
        distances.insert(source, 0.0);

        // Relax edges |V| - 1 times
        let node_count = self.adjacency.len();
        for _ in 0..node_count - 1 {
            for (&from, neighbors) in &self.adjacency {
                if distances[&from] == f64::INFINITY {
                    continue;
                }
                for &(to, weight) in neighbors {
                    let new_dist = distances[&from] + weight;
                    if new_dist < distances[&to] {
                        distances.insert(to, new_dist);
                        parents.insert(to, from);
                    }
                }
            }
        }

        // Check for negative cycles
        let mut has_negative_cycle = false;
        for (&from, neighbors) in &self.adjacency {
            if distances[&from] == f64::INFINITY {
                continue;
            }
            for &(to, weight) in neighbors {
                if distances[&from] + weight < distances[&to] {
                    has_negative_cycle = true;
                    break;
                }
            }
            if has_negative_cycle {
                break;
            }
        }

        Ok((
            ShortestPathResult {
                distances,
                parents,
                path: None,
            },
            has_negative_cycle,
        ))
    }

    /// Reconstruct path from parents map
    pub(in crate::graph::algorithms) fn reconstruct_path(
        &self,
        parents: &HashMap<u64, u64>,
        source: u64,
        target: u64,
    ) -> Option<Vec<u64>> {
        if !parents.contains_key(&target) {
            return None;
        }

        let mut path = Vec::new();
        let mut current = target;
        let mut visited = HashSet::new();

        while current != source {
            if visited.contains(&current) {
                // Cycle detected, return None
                return None;
            }
            visited.insert(current);
            path.push(current);
            current = parents[&current];
        }
        path.push(source);
        path.reverse();

        Some(path)
    }
}
