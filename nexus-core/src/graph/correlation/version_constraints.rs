//! Version Constraint Analysis for Dependencies
//!
//! Analyzes version constraints and compatibility in dependency graphs

use crate::Result;
use crate::graph::correlation::CorrelationGraph;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Version constraint type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionConstraint {
    /// Exact version (e.g., "1.0.0")
    Exact(String),
    /// Minimum version (e.g., ">=1.0.0")
    Minimum(String),
    /// Maximum version (e.g., "<2.0.0")
    Maximum(String),
    /// Range (e.g., ">=1.0.0,<2.0.0")
    Range(String, String),
    /// Caret (e.g., "^1.0.0" - compatible updates)
    Caret(String),
    /// Tilde (e.g., "~1.0.0" - patch updates)
    Tilde(String),
    /// Wildcard (e.g., "1.0.*")
    Wildcard(String),
    /// Any version
    Any,
}

impl VersionConstraint {
    /// Parse version constraint from string
    pub fn parse(constraint: &str) -> Self {
        let trimmed = constraint.trim();

        if trimmed == "*" || trimmed.is_empty() {
            return VersionConstraint::Any;
        }

        if trimmed.starts_with('^') {
            return VersionConstraint::Caret(trimmed.strip_prefix('^').unwrap().to_string());
        }

        if trimmed.starts_with('~') {
            return VersionConstraint::Tilde(trimmed.strip_prefix('~').unwrap().to_string());
        }

        if trimmed.contains('*') {
            return VersionConstraint::Wildcard(trimmed.to_string());
        }

        if trimmed.starts_with(">=") {
            if trimmed.contains(',') || trimmed.contains('<') {
                // Range like ">=1.0.0,<2.0.0"
                let parts: Vec<&str> = trimmed.split(&[',', ' '][..]).collect();
                if parts.len() >= 2 {
                    let min = parts[0].trim_start_matches(">=").trim();
                    let max = parts
                        .iter()
                        .find(|p| p.starts_with('<'))
                        .map(|p| p.trim_start_matches('<').trim())
                        .unwrap_or("");
                    return VersionConstraint::Range(min.to_string(), max.to_string());
                }
            }
            return VersionConstraint::Minimum(
                trimmed.strip_prefix(">=").unwrap().trim().to_string(),
            );
        }

        if trimmed.starts_with('<') {
            return VersionConstraint::Maximum(
                trimmed.strip_prefix('<').unwrap().trim().to_string(),
            );
        }

        // Default to exact version
        VersionConstraint::Exact(trimmed.to_string())
    }

    /// Check if a version satisfies this constraint
    pub fn satisfies(&self, version: &str) -> bool {
        match self {
            VersionConstraint::Any => true,
            VersionConstraint::Exact(v) => version == v,
            VersionConstraint::Minimum(v) => compare_versions(version, v) >= 0,
            VersionConstraint::Maximum(v) => compare_versions(version, v) < 0,
            VersionConstraint::Range(min, max) => {
                compare_versions(version, min) >= 0 && compare_versions(version, max) < 0
            }
            VersionConstraint::Caret(v) => satisfies_caret(version, v),
            VersionConstraint::Tilde(v) => satisfies_tilde(version, v),
            VersionConstraint::Wildcard(pattern) => satisfies_wildcard(version, pattern),
        }
    }
}

/// Version information for a dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyVersion {
    /// Dependency name/ID
    pub dependency_id: String,
    /// Current version
    pub current_version: String,
    /// Version constraint
    pub constraint: VersionConstraint,
    /// Available versions
    pub available_versions: Vec<String>,
}

/// Version conflict between dependencies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionConflict {
    /// Dependency with conflict
    pub dependency_id: String,
    /// Required versions by different dependents
    pub required_versions: HashMap<String, VersionConstraint>,
    /// Conflict severity
    pub severity: ConflictSeverity,
    /// Suggested resolution
    pub resolution: Option<String>,
}

/// Conflict severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictSeverity {
    /// No conflict
    None,
    /// Minor version mismatch (likely compatible)
    Minor,
    /// Major version mismatch (likely incompatible)
    Major,
    /// Critical (completely incompatible)
    Critical,
}

/// Version compatibility analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionCompatibility {
    /// All dependency versions
    pub dependencies: Vec<DependencyVersion>,
    /// Detected conflicts
    pub conflicts: Vec<VersionConflict>,
    /// Overall compatibility score (0.0 to 1.0)
    pub compatibility_score: f64,
}

/// Analyze version constraints in dependency graph
pub fn analyze_version_constraints(
    graph: &CorrelationGraph,
    version_info: &HashMap<String, DependencyVersion>,
) -> Result<VersionCompatibility> {
    let mut conflicts = Vec::new();

    // Group dependencies by their targets
    let mut dep_requirements: HashMap<String, HashMap<String, VersionConstraint>> = HashMap::new();

    for edge in &graph.edges {
        if let Some(version) = version_info.get(&edge.target) {
            dep_requirements
                .entry(edge.target.clone())
                .or_default()
                .insert(edge.source.clone(), version.constraint.clone());
        }
    }

    // Check for conflicts
    for (dep_id, requirements) in &dep_requirements {
        if requirements.len() > 1 {
            // Multiple dependents with potentially different requirements
            let conflict = check_version_conflict(dep_id, requirements);
            if conflict.severity != ConflictSeverity::None {
                conflicts.push(conflict);
            }
        }
    }

    // Calculate compatibility score
    let total_deps = version_info.len();
    let conflict_count = conflicts.len();
    let compatibility_score = if total_deps > 0 {
        1.0 - (conflict_count as f64 / total_deps as f64)
    } else {
        1.0
    };

    Ok(VersionCompatibility {
        dependencies: version_info.values().cloned().collect(),
        conflicts,
        compatibility_score,
    })
}

/// Check for version conflicts
fn check_version_conflict(
    dep_id: &str,
    requirements: &HashMap<String, VersionConstraint>,
) -> VersionConflict {
    let mut has_major_conflict = false;
    let mut has_minor_conflict = false;

    // Compare all pairs of requirements
    let req_vec: Vec<_> = requirements.iter().collect();
    for i in 0..req_vec.len() {
        for j in (i + 1)..req_vec.len() {
            let (_, constraint1) = req_vec[i];
            let (_, constraint2) = req_vec[j];

            if !constraints_compatible(constraint1, constraint2) {
                // Check if it's a major or minor conflict
                if is_major_version_conflict(constraint1, constraint2) {
                    has_major_conflict = true;
                } else {
                    has_minor_conflict = true;
                }
            }
        }
    }

    let severity = if has_major_conflict {
        ConflictSeverity::Major
    } else if has_minor_conflict {
        ConflictSeverity::Minor
    } else {
        ConflictSeverity::None
    };

    let resolution = if severity != ConflictSeverity::None {
        Some(generate_resolution_suggestion(requirements))
    } else {
        None
    };

    VersionConflict {
        dependency_id: dep_id.to_string(),
        required_versions: requirements.clone(),
        severity,
        resolution,
    }
}

/// Check if two constraints are compatible
fn constraints_compatible(c1: &VersionConstraint, c2: &VersionConstraint) -> bool {
    match (c1, c2) {
        (VersionConstraint::Any, _) | (_, VersionConstraint::Any) => true,
        (VersionConstraint::Exact(v1), VersionConstraint::Exact(v2)) => v1 == v2,
        _ => true, // Simplified - in real impl would check overlapping ranges
    }
}

/// Check if conflict is a major version difference
fn is_major_version_conflict(c1: &VersionConstraint, c2: &VersionConstraint) -> bool {
    let v1 = extract_major_version(c1);
    let v2 = extract_major_version(c2);

    match (v1, v2) {
        (Some(major1), Some(major2)) => major1 != major2,
        _ => false,
    }
}

/// Extract major version number from constraint
fn extract_major_version(constraint: &VersionConstraint) -> Option<u32> {
    let version_str = match constraint {
        VersionConstraint::Exact(v) => v,
        VersionConstraint::Minimum(v) => v,
        VersionConstraint::Maximum(v) => v,
        VersionConstraint::Range(v, _) => v,
        VersionConstraint::Caret(v) => v,
        VersionConstraint::Tilde(v) => v,
        _ => return None,
    };

    version_str.split('.').next()?.parse().ok()
}

/// Generate resolution suggestion
fn generate_resolution_suggestion(requirements: &HashMap<String, VersionConstraint>) -> String {
    format!(
        "Consider updating to a compatible version that satisfies all {} dependents",
        requirements.len()
    )
}

/// Compare two version strings (simplified semver)
fn compare_versions(v1: &str, v2: &str) -> i32 {
    let parts1: Vec<u32> = v1.split('.').filter_map(|s| s.parse().ok()).collect();
    let parts2: Vec<u32> = v2.split('.').filter_map(|s| s.parse().ok()).collect();

    for i in 0..3 {
        let p1 = parts1.get(i).copied().unwrap_or(0);
        let p2 = parts2.get(i).copied().unwrap_or(0);

        if p1 > p2 {
            return 1;
        } else if p1 < p2 {
            return -1;
        }
    }

    0
}

/// Check if version satisfies caret constraint (^)
fn satisfies_caret(version: &str, constraint: &str) -> bool {
    let v_major = version
        .split('.')
        .next()
        .and_then(|s| s.parse::<u32>().ok());
    let c_major = constraint
        .split('.')
        .next()
        .and_then(|s| s.parse::<u32>().ok());

    match (v_major, c_major) {
        (Some(vm), Some(cm)) => vm == cm && compare_versions(version, constraint) >= 0,
        _ => false,
    }
}

/// Check if version satisfies tilde constraint (~)
fn satisfies_tilde(version: &str, constraint: &str) -> bool {
    let v_parts: Vec<&str> = version.split('.').collect();
    let c_parts: Vec<&str> = constraint.split('.').collect();

    if v_parts.len() < 2 || c_parts.len() < 2 {
        return false;
    }

    // Major and minor must match
    v_parts[0] == c_parts[0]
        && v_parts[1] == c_parts[1]
        && compare_versions(version, constraint) >= 0
}

/// Check if version satisfies wildcard pattern
fn satisfies_wildcard(version: &str, pattern: &str) -> bool {
    let v_parts: Vec<&str> = version.split('.').collect();
    let p_parts: Vec<&str> = pattern.split('.').collect();

    for (i, p) in p_parts.iter().enumerate() {
        if *p == "*" {
            return true;
        }
        if i >= v_parts.len() || v_parts[i] != *p {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_exact_version() {
        let constraint = VersionConstraint::parse("1.0.0");
        assert_eq!(constraint, VersionConstraint::Exact("1.0.0".to_string()));
    }

    #[test]
    fn test_parse_caret() {
        let constraint = VersionConstraint::parse("^1.0.0");
        assert_eq!(constraint, VersionConstraint::Caret("1.0.0".to_string()));
    }

    #[test]
    fn test_parse_tilde() {
        let constraint = VersionConstraint::parse("~1.0.0");
        assert_eq!(constraint, VersionConstraint::Tilde("1.0.0".to_string()));
    }

    #[test]
    fn test_parse_range() {
        let constraint = VersionConstraint::parse(">=1.0.0,<2.0.0");
        assert_eq!(
            constraint,
            VersionConstraint::Range("1.0.0".to_string(), "2.0.0".to_string())
        );
    }

    #[test]
    fn test_satisfies_exact() {
        let constraint = VersionConstraint::Exact("1.0.0".to_string());
        assert!(constraint.satisfies("1.0.0"));
        assert!(!constraint.satisfies("1.0.1"));
    }

    #[test]
    fn test_satisfies_caret() {
        let constraint = VersionConstraint::Caret("1.0.0".to_string());
        assert!(constraint.satisfies("1.0.0"));
        assert!(constraint.satisfies("1.1.0"));
        assert!(!constraint.satisfies("2.0.0"));
    }

    #[test]
    fn test_satisfies_tilde() {
        let constraint = VersionConstraint::Tilde("1.0.0".to_string());
        assert!(constraint.satisfies("1.0.0"));
        assert!(constraint.satisfies("1.0.5"));
        assert!(!constraint.satisfies("1.1.0"));
    }

    #[test]
    fn test_compare_versions() {
        assert_eq!(compare_versions("1.0.0", "1.0.0"), 0);
        assert_eq!(compare_versions("1.0.1", "1.0.0"), 1);
        assert_eq!(compare_versions("1.0.0", "1.0.1"), -1);
        assert_eq!(compare_versions("2.0.0", "1.9.9"), 1);
    }

    #[test]
    fn test_version_conflict_detection() {
        let mut requirements = HashMap::new();
        requirements.insert(
            "app1".to_string(),
            VersionConstraint::Exact("1.0.0".to_string()),
        );
        requirements.insert(
            "app2".to_string(),
            VersionConstraint::Exact("2.0.0".to_string()),
        );

        let conflict = check_version_conflict("lib", &requirements);
        assert!(conflict.severity != ConflictSeverity::None);
    }

    #[test]
    fn test_wildcard_matching() {
        assert!(satisfies_wildcard("1.0.0", "1.0.*"));
        assert!(satisfies_wildcard("1.0.5", "1.0.*"));
        assert!(!satisfies_wildcard("1.1.0", "1.0.*"));
        assert!(satisfies_wildcard("1.2.3", "1.*"));
    }
}
