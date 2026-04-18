//! Geospatial support for Nexus
//!
//! This module provides:
//! - Point data type (2D and 3D coordinates)
//! - Distance functions
//! - Spatial operations
//! - Geospatial procedures
//!
//! # Examples
//!
//! ```rust
//! use nexus_core::geospatial::{Point, CoordinateSystem};
//!
//! // Create a 2D point in Cartesian coordinates
//! let p1 = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
//!
//! // Create a 3D point in WGS84 coordinates
//! let p2 = Point::new_3d(-122.4194, 37.7749, 0.0, CoordinateSystem::WGS84);
//!
//! // Calculate distance
//! let distance = p1.distance_to(&p2);
//! ```

pub mod procedures;
pub mod rtree;

use serde::{Deserialize, Serialize};

/// Coordinate system for points
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoordinateSystem {
    /// Cartesian coordinate system (x, y, z)
    Cartesian,
    /// WGS84 geographic coordinate system (longitude, latitude, height)
    WGS84,
}

/// Point in 2D or 3D space
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// X coordinate (or longitude for WGS84)
    pub x: f64,
    /// Y coordinate (or latitude for WGS84)
    pub y: f64,
    /// Z coordinate (or height for WGS84), None for 2D points
    pub z: Option<f64>,
    /// Coordinate system
    pub coordinate_system: CoordinateSystem,
}

impl Point {
    /// Create a new 2D point
    pub fn new_2d(x: f64, y: f64, coordinate_system: CoordinateSystem) -> Self {
        Self {
            x,
            y,
            z: None,
            coordinate_system,
        }
    }

    /// Create a new 3D point
    pub fn new_3d(x: f64, y: f64, z: f64, coordinate_system: CoordinateSystem) -> Self {
        Self {
            x,
            y,
            z: Some(z),
            coordinate_system,
        }
    }

    /// Check if this is a 2D point
    pub fn is_2d(&self) -> bool {
        self.z.is_none()
    }

    /// Check if this is a 3D point
    pub fn is_3d(&self) -> bool {
        self.z.is_some()
    }

    /// Get the Z coordinate (or height), returns 0.0 for 2D points
    pub fn z(&self) -> f64 {
        self.z.unwrap_or(0.0)
    }

    /// Calculate distance to another point
    ///
    /// For Cartesian coordinates, uses Euclidean distance.
    /// For WGS84 coordinates, uses Haversine formula for great-circle distance.
    pub fn distance_to(&self, other: &Point) -> f64 {
        // Both points must use the same coordinate system
        if self.coordinate_system != other.coordinate_system {
            // For different coordinate systems, convert to Cartesian and calculate
            // This is a simplified approach - in production, proper coordinate transformation would be needed
            let dx = self.x - other.x;
            let dy = self.y - other.y;
            let dz = self.z() - other.z();
            (dx * dx + dy * dy + dz * dz).sqrt()
        } else {
            match self.coordinate_system {
                CoordinateSystem::Cartesian => {
                    let dx = self.x - other.x;
                    let dy = self.y - other.y;
                    let dz = self.z() - other.z();
                    (dx * dx + dy * dy + dz * dz).sqrt()
                }
                CoordinateSystem::WGS84 => {
                    // Haversine formula for great-circle distance
                    let lat1 = self.y.to_radians();
                    let lat2 = other.y.to_radians();
                    let lon1 = self.x.to_radians();
                    let lon2 = other.x.to_radians();

                    let dlat = lat2 - lat1;
                    let dlon = lon2 - lon1;

                    let a = (dlat / 2.0).sin().powi(2)
                        + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
                    let c = 2.0 * a.sqrt().asin();

                    // Earth radius in meters
                    let earth_radius_meters = 6371000.0;
                    let distance_meters = earth_radius_meters * c;

                    // If both points have height, add vertical distance
                    if let (Some(z1), Some(z2)) = (self.z, other.z) {
                        let dz = z1 - z2;
                        (distance_meters * distance_meters + dz * dz).sqrt()
                    } else {
                        distance_meters
                    }
                }
            }
        }
    }

    /// Convert to serde_json::Value for use in Cypher queries
    pub fn to_json_value(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert(
            "x".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(self.x).unwrap_or_else(|| serde_json::Number::from(0)),
            ),
        );
        map.insert(
            "y".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(self.y).unwrap_or_else(|| serde_json::Number::from(0)),
            ),
        );
        if let Some(z) = self.z {
            map.insert(
                "z".to_string(),
                serde_json::Value::Number(
                    serde_json::Number::from_f64(z).unwrap_or_else(|| serde_json::Number::from(0)),
                ),
            );
        }
        map.insert(
            "crs".to_string(),
            serde_json::Value::String(match (self.coordinate_system, self.z.is_some()) {
                (CoordinateSystem::Cartesian, true) => "cartesian-3d".to_string(),
                (CoordinateSystem::Cartesian, false) => "cartesian".to_string(),
                (CoordinateSystem::WGS84, true) => "wgs-84-3d".to_string(),
                (CoordinateSystem::WGS84, false) => "wgs-84".to_string(),
            }),
        );
        serde_json::Value::Object(map)
    }

    /// Create from serde_json::Value (from Cypher query result)
    pub fn from_json_value(value: &serde_json::Value) -> Result<Self, String> {
        let obj = value
            .as_object()
            .ok_or_else(|| "Point must be an object".to_string())?;

        // Support both x/y and longitude/latitude aliases
        let x = obj
            .get("x")
            .or_else(|| obj.get("longitude"))
            .and_then(|v| v.as_f64())
            .ok_or_else(|| "Missing or invalid 'x' or 'longitude' coordinate".to_string())?;

        let y = obj
            .get("y")
            .or_else(|| obj.get("latitude"))
            .and_then(|v| v.as_f64())
            .ok_or_else(|| "Missing or invalid 'y' or 'latitude' coordinate".to_string())?;

        let z = obj.get("z").and_then(|v| v.as_f64());

        let crs_str = obj
            .get("crs")
            .and_then(|v| v.as_str())
            .unwrap_or("cartesian");

        let coordinate_system = match crs_str {
            "cartesian" | "cartesian-3d" => CoordinateSystem::Cartesian,
            "wgs-84" | "wgs-84-3d" => CoordinateSystem::WGS84,
            _ => CoordinateSystem::Cartesian, // Default to Cartesian
        };

        Ok(if let Some(z_val) = z {
            Self::new_3d(x, y, z_val, coordinate_system)
        } else {
            Self::new_2d(x, y, coordinate_system)
        })
    }
}

impl std::fmt::Display for Point {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(z) = self.z {
            write!(
                f,
                "point({{x: {}, y: {}, z: {}, crs: {:?}}})",
                self.x, self.y, z, self.coordinate_system
            )
        } else {
            write!(
                f,
                "point({{x: {}, y: {}, crs: {:?}}})",
                self.x, self.y, self.coordinate_system
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_2d_creation() {
        let p = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
        assert_eq!(p.z, None);
        assert!(p.is_2d());
        assert!(!p.is_3d());
    }

    #[test]
    fn test_point_3d_creation() {
        let p = Point::new_3d(1.0, 2.0, 3.0, CoordinateSystem::Cartesian);
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
        assert_eq!(p.z, Some(3.0));
        assert!(!p.is_2d());
        assert!(p.is_3d());
    }

    #[test]
    fn test_point_distance_cartesian_2d() {
        let p1 = Point::new_2d(0.0, 0.0, CoordinateSystem::Cartesian);
        let p2 = Point::new_2d(3.0, 4.0, CoordinateSystem::Cartesian);
        let distance = p1.distance_to(&p2);
        assert!((distance - 5.0).abs() < 0.0001);
    }

    #[test]
    fn test_point_distance_cartesian_3d() {
        let p1 = Point::new_3d(0.0, 0.0, 0.0, CoordinateSystem::Cartesian);
        let p2 = Point::new_3d(2.0, 3.0, 6.0, CoordinateSystem::Cartesian);
        let distance = p1.distance_to(&p2);
        assert!((distance - 7.0).abs() < 0.0001); // sqrt(4 + 9 + 36) = sqrt(49) = 7
    }

    #[test]
    fn test_point_distance_wgs84() {
        // San Francisco to New York (approximate)
        let sf = Point::new_2d(-122.4194, 37.7749, CoordinateSystem::WGS84);
        let ny = Point::new_2d(-74.0060, 40.7128, CoordinateSystem::WGS84);
        let distance = sf.distance_to(&ny);
        // Should be approximately 4139 km
        assert!(distance > 4000000.0 && distance < 4300000.0);
    }

    #[test]
    fn test_point_to_json() {
        let p = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
        let json = p.to_json_value();
        assert_eq!(json["x"].as_f64().unwrap(), 1.0);
        assert_eq!(json["y"].as_f64().unwrap(), 2.0);
        assert_eq!(json["crs"].as_str().unwrap(), "cartesian");
    }

    #[test]
    fn test_point_from_json() {
        let json = serde_json::json!({
            "x": 1.0,
            "y": 2.0,
            "crs": "cartesian"
        });
        let p = Point::from_json_value(&json).unwrap();
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
        assert_eq!(p.z, None);
        assert_eq!(p.coordinate_system, CoordinateSystem::Cartesian);
    }

    #[test]
    fn test_point_from_json_3d() {
        let json = serde_json::json!({
            "x": 1.0,
            "y": 2.0,
            "z": 3.0,
            "crs": "cartesian-3d"
        });
        let p = Point::from_json_value(&json).unwrap();
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
        assert_eq!(p.z, Some(3.0));
        assert_eq!(p.coordinate_system, CoordinateSystem::Cartesian);
    }

    #[test]
    fn test_point_distance_same_point() {
        let p1 = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
        let p2 = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
        let distance = p1.distance_to(&p2);
        assert!((distance - 0.0).abs() < 0.0001);
    }

    #[test]
    fn test_point_distance_wgs84_same_location() {
        let p1 = Point::new_2d(-122.4194, 37.7749, CoordinateSystem::WGS84);
        let p2 = Point::new_2d(-122.4194, 37.7749, CoordinateSystem::WGS84);
        let distance = p1.distance_to(&p2);
        assert!((distance - 0.0).abs() < 1.0); // Allow small floating point error
    }
}
