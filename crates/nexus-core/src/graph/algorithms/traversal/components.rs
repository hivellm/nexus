//! Connected-components and community-detection algorithms.

use super::super::*;

impl Graph {
    /// Find connected components using Union-Find
    pub fn connected_components(&self) -> ConnectedComponentsResult {
        let mut components = HashMap::new();
        let mut component_count = 0;
        let mut visited = HashSet::new();

        for &node in self.adjacency.keys() {
            if !visited.contains(&node) {
                let mut component_nodes = Vec::new();
                self.dfs_component(node, &mut visited, &mut component_nodes);

                for &component_node in &component_nodes {
                    components.insert(component_node, component_count);
                }
                component_count += 1;
            }
        }

        // Group nodes by component
        let mut component_nodes = HashMap::new();
        for (node, component_id) in &components {
            component_nodes
                .entry(*component_id)
                .or_insert_with(Vec::new)
                .push(*node);
        }

        ConnectedComponentsResult {
            components,
            component_count,
            component_nodes,
        }
    }

    /// Find strongly connected components using Kosaraju's algorithm
    /// Returns component ID for each node
    pub fn strongly_connected_components(&self) -> ConnectedComponentsResult {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let mut visited = HashSet::new();
        let mut finish_order = Vec::new();

        // First DFS pass - get finish times
        for &node in &nodes {
            if !visited.contains(&node) {
                self.dfs_finish_order(node, &mut visited, &mut finish_order);
            }
        }

        // Build reverse graph
        let mut reverse_adjacency: HashMap<u64, Vec<u64>> = HashMap::new();
        for &node in &nodes {
            reverse_adjacency.insert(node, Vec::new());
        }
        for (&from, neighbors) in &self.adjacency {
            for &(to, _) in neighbors {
                reverse_adjacency.entry(to).or_default().push(from);
            }
        }

        // Second DFS pass on reverse graph
        visited.clear();
        let mut components = HashMap::new();
        let mut component_id = 0;
        let mut component_nodes = HashMap::new();

        for &node in finish_order.iter().rev() {
            if !visited.contains(&node) {
                let mut component = Vec::new();
                self.dfs_component_reverse(node, &reverse_adjacency, &mut visited, &mut component);
                for &comp_node in &component {
                    components.insert(comp_node, component_id);
                }
                component_nodes.insert(component_id, component);
                component_id += 1;
            }
        }

        ConnectedComponentsResult {
            components,
            component_count: component_id,
            component_nodes,
        }
    }

    /// Label Propagation Algorithm for community detection
    /// Returns community ID for each node
    pub fn label_propagation(&self, max_iterations: usize) -> ConnectedComponentsResult {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let mut labels: HashMap<u64, usize> = HashMap::new();

        // Initialize each node with its own label
        for (idx, &node) in nodes.iter().enumerate() {
            labels.insert(node, idx);
        }

        for _ in 0..max_iterations {
            let mut new_labels = labels.clone();
            let mut changed = false;

            // Shuffle nodes for random order
            let mut shuffled_nodes = nodes.clone();
            // Simple pseudo-shuffle (in real implementation, use proper RNG)
            for i in 0..shuffled_nodes.len() {
                let j = (i * 7 + 13) % shuffled_nodes.len();
                shuffled_nodes.swap(i, j);
            }

            for &node in &shuffled_nodes {
                let mut label_counts: HashMap<usize, usize> = HashMap::new();

                // Count labels of neighbors
                for &(neighbor, _) in self.get_neighbors(node) {
                    let neighbor_label = labels[&neighbor];
                    *label_counts.entry(neighbor_label).or_default() += 1;
                }

                // Also count self
                let self_label = labels[&node];
                *label_counts.entry(self_label).or_default() += 1;

                // Choose most frequent label
                if let Some((&most_frequent_label, _)) =
                    label_counts.iter().max_by_key(|&(_, count)| count)
                {
                    if most_frequent_label != labels[&node] {
                        new_labels.insert(node, most_frequent_label);
                        changed = true;
                    }
                }
            }

            labels = new_labels;
            if !changed {
                break;
            }
        }

        // Convert to ConnectedComponentsResult format
        let mut component_map = HashMap::new();
        let mut component_nodes_map = HashMap::new();
        let mut label_to_component: HashMap<usize, usize> = HashMap::new();
        let mut component_id = 0;

        for &node in &nodes {
            let label = labels[&node];
            let comp_id = *label_to_component.entry(label).or_insert_with(|| {
                let id = component_id;
                component_nodes_map.insert(id, Vec::new());
                component_id += 1;
                id
            });
            component_map.insert(node, comp_id);
            component_nodes_map.get_mut(&comp_id).unwrap().push(node);
        }

        ConnectedComponentsResult {
            components: component_map,
            component_count: component_id,
            component_nodes: component_nodes_map,
        }
    }

    /// Louvain algorithm for community detection (simplified version)
    /// Returns community ID for each node
    pub fn louvain(&self, max_iterations: usize) -> ConnectedComponentsResult {
        // Simplified Louvain implementation
        // Full implementation would optimize modularity more carefully
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let mut communities: HashMap<u64, usize> = HashMap::new();

        // Initialize each node in its own community
        for (idx, &node) in nodes.iter().enumerate() {
            communities.insert(node, idx);
        }

        for _ in 0..max_iterations {
            let mut changed = false;
            let mut shuffled_nodes = nodes.clone();
            // Simple pseudo-shuffle
            for i in 0..shuffled_nodes.len() {
                let j = (i * 7 + 13) % shuffled_nodes.len();
                shuffled_nodes.swap(i, j);
            }

            for &node in &shuffled_nodes {
                let mut best_community = communities[&node];
                let mut best_modularity_gain = 0.0;

                // Try moving to each neighbor's community
                for &(neighbor, _) in self.get_neighbors(node) {
                    let neighbor_community = communities[&neighbor];
                    let gain =
                        self.calculate_modularity_gain(node, neighbor_community, &communities);
                    if gain > best_modularity_gain {
                        best_modularity_gain = gain;
                        best_community = neighbor_community;
                    }
                }

                if best_community != communities[&node] {
                    communities.insert(node, best_community);
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }

        // Convert to ConnectedComponentsResult format
        let mut component_map = HashMap::new();
        let mut component_nodes_map = HashMap::new();
        let mut community_to_component: HashMap<usize, usize> = HashMap::new();
        let mut component_id = 0;

        for &node in &nodes {
            let community = communities[&node];
            let comp_id = *community_to_component.entry(community).or_insert_with(|| {
                let id = component_id;
                component_nodes_map.insert(id, Vec::new());
                component_id += 1;
                id
            });
            component_map.insert(node, comp_id);
            component_nodes_map.get_mut(&comp_id).unwrap().push(node);
        }

        ConnectedComponentsResult {
            components: component_map,
            component_count: component_id,
            component_nodes: component_nodes_map,
        }
    }

    /// Calculate modularity gain for moving a node to a community
    fn calculate_modularity_gain(
        &self,
        node: u64,
        community: usize,
        communities: &HashMap<u64, usize>,
    ) -> f64 {
        // Simplified modularity gain calculation
        // Count edges within community vs edges outside
        let mut edges_in_community = 0;
        let mut edges_outside = 0;

        for &(neighbor, _) in self.get_neighbors(node) {
            if communities.get(&neighbor) == Some(&community) {
                edges_in_community += 1;
            } else {
                edges_outside += 1;
            }
        }

        // Simple heuristic: prefer communities with more connections
        edges_in_community as f64 - edges_outside as f64 * 0.5
    }
}
