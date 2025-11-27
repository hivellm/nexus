//! R-tree spatial index for geospatial queries
//!
//! This module provides an R-tree implementation for efficient spatial queries
//! on Point data. The R-tree organizes points in a hierarchy of bounding boxes
//! for fast range queries and nearest neighbor searches.
//!
//! # Example
//!
//! ```rust
//! use nexus_core::geospatial::{Point, CoordinateSystem};
//! use nexus_core::geospatial::rtree::RTreeIndex;
//!
//! let mut index = RTreeIndex::new();
//! let point = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
//! index.insert(1, &point);
//!
//! // Query points within bounding box
//! let bbox = (0.0, 0.0, 10.0, 10.0); // min_x, min_y, max_x, max_y
//! let results = index.query_bbox(bbox);
//! ```

use crate::geospatial::Point;
use crate::{Error, Result};
use parking_lot::RwLock;
use roaring::RoaringBitmap;
use std::collections::HashMap;
use std::sync::Arc;

/// R-tree spatial index for Point data
///
/// Organizes points in a hierarchy of bounding boxes for efficient spatial queries.
/// Currently implements a simple grid-based approach for MVP.
#[derive(Clone)]
pub struct RTreeIndex {
    /// Mapping from node_id to Point
    points: Arc<RwLock<HashMap<u64, Point>>>,
    /// Grid-based spatial index (simplified R-tree)
    /// Maps grid cell (x_cell, y_cell) to set of node_ids
    grid: Arc<RwLock<HashMap<(i32, i32), RoaringBitmap>>>,
    /// Grid cell size (for spatial partitioning)
    cell_size: f64,
    /// Statistics
    stats: Arc<RwLock<RTreeIndexStats>>,
}

/// Statistics for R-tree index
#[derive(Debug, Clone, Default)]
pub struct RTreeIndexStats {
    /// Total number of points indexed
    pub total_points: u64,
    /// Number of grid cells used
    pub grid_cells: u32,
    /// Average points per cell
    pub avg_points_per_cell: f64,
}

impl RTreeIndex {
    /// Create a new R-tree index with default cell size
    pub fn new() -> Self {
        Self::with_cell_size(100.0) // Default cell size of 100 units
    }

    /// Create a new R-tree index with custom cell size
    pub fn with_cell_size(cell_size: f64) -> Self {
        Self {
            points: Arc::new(RwLock::new(HashMap::new())),
            grid: Arc::new(RwLock::new(HashMap::new())),
            cell_size,
            stats: Arc::new(RwLock::new(RTreeIndexStats::default())),
        }
    }

    /// Insert a point for a node
    pub fn insert(&self, node_id: u64, point: &Point) -> Result<()> {
        let mut points = self.points.write();
        let mut grid = self.grid.write();
        let mut stats = self.stats.write();

        // Store point (clone since Point doesn't implement Copy)
        points.insert(node_id, point.clone());

        // Calculate grid cell
        let cell_x = (point.x / self.cell_size).floor() as i32;
        let cell_y = (point.y / self.cell_size).floor() as i32;

        // Add to grid cell
        grid.entry((cell_x, cell_y))
            .or_default()
            .insert(node_id as u32);

        // Update statistics
        stats.total_points = points.len() as u64;
        stats.grid_cells = grid.len() as u32;
        stats.avg_points_per_cell = if stats.grid_cells > 0 {
            stats.total_points as f64 / stats.grid_cells as f64
        } else {
            0.0
        };

        Ok(())
    }

    /// Remove a point for a node
    pub fn remove(&self, node_id: u64) -> Result<()> {
        let mut points = self.points.write();
        let mut grid = self.grid.write();
        let mut stats = self.stats.write();

        // Remove point
        if let Some(point) = points.remove(&node_id) {
            // Calculate grid cell
            let cell_x = (point.x / self.cell_size).floor() as i32;
            let cell_y = (point.y / self.cell_size).floor() as i32;

            // Remove from grid cell
            if let Some(cell) = grid.get_mut(&(cell_x, cell_y)) {
                cell.remove(node_id as u32);
                if cell.is_empty() {
                    grid.remove(&(cell_x, cell_y));
                }
            }

            // Update statistics
            stats.total_points = points.len() as u64;
            stats.grid_cells = grid.len() as u32;
            stats.avg_points_per_cell = if stats.grid_cells > 0 {
                stats.total_points as f64 / stats.grid_cells as f64
            } else {
                0.0
            };
        }

        Ok(())
    }

    /// Query points within a bounding box
    ///
    /// # Arguments
    /// * `bbox` - Bounding box as (min_x, min_y, max_x, max_y)
    ///
    /// # Returns
    /// Bitmap of node_ids within the bounding box
    pub fn query_bbox(&self, bbox: (f64, f64, f64, f64)) -> Result<RoaringBitmap> {
        let (min_x, min_y, max_x, max_y) = bbox;
        let points = self.points.read();
        let grid = self.grid.read();

        // Calculate grid cells that intersect with bounding box
        let min_cell_x = (min_x / self.cell_size).floor() as i32;
        let min_cell_y = (min_y / self.cell_size).floor() as i32;
        let max_cell_x = (max_x / self.cell_size).floor() as i32;
        let max_cell_y = (max_y / self.cell_size).floor() as i32;

        let mut result = RoaringBitmap::new();

        // Check all grid cells that intersect with bounding box
        for cell_x in min_cell_x..=max_cell_x {
            for cell_y in min_cell_y..=max_cell_y {
                if let Some(cell_nodes) = grid.get(&(cell_x, cell_y)) {
                    // Add all nodes from this cell
                    result |= cell_nodes;
                }
            }
        }

        // Filter nodes that are actually within bounding box (refinement step)
        let mut filtered_result = RoaringBitmap::new();
        for node_id in result.iter() {
            if let Some(point) = points.get(&(node_id as u64)) {
                if point.x >= min_x && point.x <= max_x && point.y >= min_y && point.y <= max_y {
                    filtered_result.insert(node_id);
                }
            }
        }

        Ok(filtered_result)
    }

    /// Query points within a distance from a center point
    ///
    /// # Arguments
    /// * `center` - Center point
    /// * `max_distance` - Maximum distance
    ///
    /// # Returns
    /// Bitmap of node_ids within the distance
    pub fn query_distance(&self, center: &Point, max_distance: f64) -> Result<RoaringBitmap> {
        let points = self.points.read();

        // Create bounding box around center point
        let bbox = (
            center.x - max_distance,
            center.y - max_distance,
            center.x + max_distance,
            center.y + max_distance,
        );

        // Get candidates from bounding box query
        let candidates = self.query_bbox(bbox)?;

        // Filter by actual distance
        let mut result = RoaringBitmap::new();
        for node_id in candidates.iter() {
            if let Some(point) = points.get(&(node_id as u64)) {
                let distance = center.distance_to(point);
                if distance <= max_distance {
                    result.insert(node_id);
                }
            }
        }

        Ok(result)
    }

    /// Get statistics
    pub fn get_stats(&self) -> RTreeIndexStats {
        self.stats.read().clone()
    }

    /// Check if a node has a point
    pub fn has_point(&self, node_id: u64) -> bool {
        let points = self.points.read();
        points.contains_key(&node_id)
    }

    /// Get point for a node
    pub fn get_point(&self, node_id: u64) -> Option<Point> {
        let points = self.points.read();
        points.get(&node_id).cloned()
    }

    /// Clear all data
    pub fn clear(&mut self) -> Result<()> {
        let mut points = self.points.write();
        let mut grid = self.grid.write();
        let mut stats = self.stats.write();

        points.clear();
        grid.clear();
        *stats = RTreeIndexStats::default();

        Ok(())
    }

    /// Health check for the R-tree index
    pub fn health_check(&self) -> Result<()> {
        let stats = self.stats.read();

        // Check if the total points count is reasonable
        if stats.total_points > 1_000_000_000 {
            // 1 billion max
            return Err(Error::index("Too many points in R-tree index"));
        }

        Ok(())
    }
}

impl Default for RTreeIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geospatial::CoordinateSystem;

    #[test]
    fn test_rtree_creation() {
        let index = RTreeIndex::new();
        let stats = index.get_stats();
        assert_eq!(stats.total_points, 0);
        assert_eq!(stats.grid_cells, 0);
    }

    #[test]
    fn test_rtree_insert() {
        let index = RTreeIndex::new();
        let point = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);

        index.insert(1, &point).unwrap();

        let stats = index.get_stats();
        assert_eq!(stats.total_points, 1);
        assert!(index.has_point(1));
        assert_eq!(index.get_point(1), Some(point));
    }

    #[test]
    fn test_rtree_remove() {
        let index = RTreeIndex::new();
        let point = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);

        index.insert(1, &point).unwrap();
        assert!(index.has_point(1));

        index.remove(1).unwrap();
        assert!(!index.has_point(1));

        let stats = index.get_stats();
        assert_eq!(stats.total_points, 0);
    }

    #[test]
    fn test_rtree_query_bbox() {
        let index = RTreeIndex::new();

        // Insert points
        index
            .insert(1, &Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian))
            .unwrap();
        index
            .insert(2, &Point::new_2d(15.0, 15.0, CoordinateSystem::Cartesian))
            .unwrap();
        index
            .insert(3, &Point::new_2d(25.0, 25.0, CoordinateSystem::Cartesian))
            .unwrap();

        // Query bounding box (0, 0, 10, 10)
        let results = index.query_bbox((0.0, 0.0, 10.0, 10.0)).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results.contains(1));
        assert!(!results.contains(2));
        assert!(!results.contains(3));
    }

    #[test]
    fn test_rtree_query_distance() {
        let index = RTreeIndex::new();

        // Insert points
        index
            .insert(1, &Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian))
            .unwrap();
        index
            .insert(2, &Point::new_2d(15.0, 15.0, CoordinateSystem::Cartesian))
            .unwrap();
        index
            .insert(3, &Point::new_2d(25.0, 25.0, CoordinateSystem::Cartesian))
            .unwrap();

        // Query points within distance 10 from (5, 5)
        let center = Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian);
        let results = index.query_distance(&center, 10.0).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results.contains(1));
        assert!(!results.contains(2));
        assert!(!results.contains(3));
    }

    #[test]
    fn test_rtree_clear() {
        let mut index = RTreeIndex::new();

        index
            .insert(1, &Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian))
            .unwrap();
        index
            .insert(2, &Point::new_2d(3.0, 4.0, CoordinateSystem::Cartesian))
            .unwrap();

        assert_eq!(index.get_stats().total_points, 2);

        index.clear().unwrap();

        assert_eq!(index.get_stats().total_points, 0);
        assert!(!index.has_point(1));
        assert!(!index.has_point(2));
    }

    #[test]
    fn test_rtree_health_check() {
        let index = RTreeIndex::new();

        // Empty index should pass health check
        index.health_check().unwrap();

        // Add reasonable amount of data
        for i in 0..1000 {
            let point = Point::new_2d(i as f64, i as f64, CoordinateSystem::Cartesian);
            index.insert(i, &point).unwrap();
        }
        index.health_check().unwrap();
    }
}
