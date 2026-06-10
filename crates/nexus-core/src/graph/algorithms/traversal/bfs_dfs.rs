//! BFS, DFS, topological sort, and related helpers.

use super::super::*;

impl Graph {
    /// Perform Breadth-First Search from a source node
    pub fn bfs(&self, source: u64) -> Result<BfsResult> {
        if !self.has_node(source) {
            return Err(Error::InvalidInput("Source node not found".to_string()));
        }

        let mut distances = HashMap::new();
        let mut parents = HashMap::new();
        let mut discovery_order = Vec::new();
        let mut queue = VecDeque::new();

        distances.insert(source, 0);
        queue.push_back(source);
        discovery_order.push(source);

        while let Some(current) = queue.pop_front() {
            let current_distance = distances[&current];

            for &(neighbor, _) in self.get_neighbors(current) {
                if let std::collections::hash_map::Entry::Vacant(e) = distances.entry(neighbor) {
                    e.insert(current_distance + 1);
                    parents.insert(neighbor, current);
                    queue.push_back(neighbor);
                    discovery_order.push(neighbor);
                }
            }
        }

        Ok(BfsResult {
            distances,
            parents,
            discovery_order,
        })
    }

    /// Perform Depth-First Search from a source node
    pub fn dfs(&self, source: u64) -> Result<DfsResult> {
        if !self.has_node(source) {
            return Err(Error::InvalidInput("Source node not found".to_string()));
        }

        let mut discovery_times = HashMap::new();
        let mut finish_times = HashMap::new();
        let mut parents = HashMap::new();
        let mut discovery_order = Vec::new();
        let mut time = 0;

        self.dfs_visit(
            source,
            &mut discovery_times,
            &mut finish_times,
            &mut parents,
            &mut discovery_order,
            &mut time,
        );

        Ok(DfsResult {
            discovery_times,
            finish_times,
            parents,
            discovery_order,
        })
    }

    /// DFS visit helper function
    fn dfs_visit(
        &self,
        node: u64,
        discovery_times: &mut HashMap<u64, usize>,
        finish_times: &mut HashMap<u64, usize>,
        parents: &mut HashMap<u64, u64>,
        discovery_order: &mut Vec<u64>,
        time: &mut usize,
    ) {
        *time += 1;
        discovery_times.insert(node, *time);
        discovery_order.push(node);

        for &(neighbor, _) in self.get_neighbors(node) {
            if !discovery_times.contains_key(&neighbor) {
                parents.insert(neighbor, node);
                self.dfs_visit(
                    neighbor,
                    discovery_times,
                    finish_times,
                    parents,
                    discovery_order,
                    time,
                );
            }
        }

        *time += 1;
        finish_times.insert(node, *time);
    }

    /// DFS helper for connected components
    /// For directed graphs, treat edges as bidirectional (undirected)
    pub(in crate::graph::algorithms) fn dfs_component(
        &self,
        node: u64,
        visited: &mut HashSet<u64>,
        component_nodes: &mut Vec<u64>,
    ) {
        visited.insert(node);
        component_nodes.push(node);

        // Follow outgoing edges
        for &(neighbor, _) in self.get_neighbors(node) {
            if !visited.contains(&neighbor) {
                self.dfs_component(neighbor, visited, component_nodes);
            }
        }

        // Also follow incoming edges (treat as undirected graph)
        for (from_node, neighbors) in &self.adjacency {
            if neighbors.iter().any(|(to, _)| *to == node) && !visited.contains(from_node) {
                self.dfs_component(*from_node, visited, component_nodes);
            }
        }
    }

    /// Perform topological sort using DFS
    pub fn topological_sort(&self) -> TopologicalSortResult {
        let mut visited = HashSet::new();
        let mut temp_visited = HashSet::new();
        let mut sorted_nodes = Vec::new();
        let mut has_cycle = false;

        for &node in self.adjacency.keys() {
            if !visited.contains(&node)
                && self.dfs_topological(
                    node,
                    &mut visited,
                    &mut temp_visited,
                    &mut sorted_nodes,
                    &mut has_cycle,
                )
            {
                has_cycle = true;
                break;
            }
        }

        sorted_nodes.reverse();

        TopologicalSortResult {
            sorted_nodes,
            has_cycle,
        }
    }

    /// DFS helper for topological sort
    fn dfs_topological(
        &self,
        node: u64,
        visited: &mut HashSet<u64>,
        temp_visited: &mut HashSet<u64>,
        sorted_nodes: &mut Vec<u64>,
        has_cycle: &mut bool,
    ) -> bool {
        if temp_visited.contains(&node) {
            *has_cycle = true;
            return true;
        }

        if visited.contains(&node) {
            return false;
        }

        temp_visited.insert(node);

        for &(neighbor, _) in self.get_neighbors(node) {
            if self.dfs_topological(neighbor, visited, temp_visited, sorted_nodes, has_cycle) {
                return true;
            }
        }

        temp_visited.remove(&node);
        visited.insert(node);
        sorted_nodes.push(node);

        false
    }

    /// DFS helper for finish order (first pass of Kosaraju)
    pub(in crate::graph::algorithms) fn dfs_finish_order(
        &self,
        node: u64,
        visited: &mut HashSet<u64>,
        finish_order: &mut Vec<u64>,
    ) {
        visited.insert(node);
        for &(neighbor, _) in self.get_neighbors(node) {
            if !visited.contains(&neighbor) {
                self.dfs_finish_order(neighbor, visited, finish_order);
            }
        }
        finish_order.push(node);
    }

    /// DFS helper for reverse graph (second pass of Kosaraju)
    pub(in crate::graph::algorithms) fn dfs_component_reverse(
        &self,
        node: u64,
        reverse_adjacency: &HashMap<u64, Vec<u64>>,
        visited: &mut HashSet<u64>,
        component: &mut Vec<u64>,
    ) {
        visited.insert(node);
        component.push(node);
        if let Some(neighbors) = reverse_adjacency.get(&node) {
            for &neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    self.dfs_component_reverse(neighbor, reverse_adjacency, visited, component);
                }
            }
        }
    }
}
