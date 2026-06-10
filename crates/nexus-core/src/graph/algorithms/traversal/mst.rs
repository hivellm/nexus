//! Minimum spanning tree (Kruskal's algorithm).

use super::super::*;

impl Graph {
    /// Find Minimum Spanning Tree using Kruskal's algorithm
    pub fn minimum_spanning_tree(&self) -> Result<MstResult> {
        let mut edges = Vec::new();

        // Collect all edges
        for (&from, neighbors) in &self.adjacency {
            for &(to, weight) in neighbors {
                if from < to {
                    // Avoid duplicate edges
                    edges.push((from, to, weight));
                }
            }
        }

        // Sort edges by weight
        edges.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal));

        // Union-Find data structure
        let mut parent = HashMap::new();
        let mut rank = HashMap::new();

        for &node in self.adjacency.keys() {
            parent.insert(node, node);
            rank.insert(node, 0);
        }

        fn find(parent: &mut HashMap<u64, u64>, x: u64) -> u64 {
            if parent[&x] != x {
                let parent_x = parent[&x];
                let root = find(parent, parent_x);
                parent.insert(x, root);
            }
            parent[&x]
        }

        fn union(
            parent: &mut HashMap<u64, u64>,
            rank: &mut HashMap<u64, u64>,
            x: u64,
            y: u64,
        ) -> bool {
            let px = find(parent, x);
            let py = find(parent, y);

            if px == py {
                return false;
            }

            if rank[&px] < rank[&py] {
                parent.insert(px, py);
            } else if rank[&px] > rank[&py] {
                parent.insert(py, px);
            } else {
                parent.insert(py, px);
                rank.insert(px, rank[&px] + 1);
            }

            true
        }

        let mut mst_edges = Vec::new();
        let mut total_weight = 0.0;

        for (from, to, weight) in edges {
            if union(&mut parent, &mut rank, from, to) {
                mst_edges.push((from, to, weight));
                total_weight += weight;
            }
        }

        Ok(MstResult {
            edges: mst_edges,
            total_weight,
        })
    }
}
