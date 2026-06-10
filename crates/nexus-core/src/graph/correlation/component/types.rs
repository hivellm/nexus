//! Component Analysis Module
//!
//! Provides advanced component analysis capabilities for object-oriented code:
//! - Class and interface analysis
//! - Inheritance and composition tracking
//! - Object-oriented hierarchy layout
//! - Interface implementation analysis
//! - Component relationship visualization
//! - Component coupling analysis
//! - Component metrics calculation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Class information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassInfo {
    /// Class name
    pub name: String,
    /// File where class is defined
    pub file: String,
    /// Line number where class is defined
    pub line: usize,
    /// Base class (if any)
    pub base_class: Option<String>,
    /// Interfaces implemented by this class
    pub interfaces: Vec<String>,
    /// Methods in this class
    pub methods: Vec<MethodInfo>,
    /// Fields/properties in this class
    pub fields: Vec<FieldInfo>,
    /// Whether class is abstract
    pub is_abstract: bool,
    /// Whether class is final/sealed
    pub is_final: bool,
    /// Access modifier (public, private, protected)
    pub access_modifier: String,
}

/// Interface information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceInfo {
    /// Interface name
    pub name: String,
    /// File where interface is defined
    pub file: String,
    /// Line number where interface is defined
    pub line: usize,
    /// Parent interfaces (if any)
    pub parent_interfaces: Vec<String>,
    /// Methods declared in this interface
    pub methods: Vec<MethodInfo>,
    /// Properties declared in this interface
    pub properties: Vec<PropertyInfo>,
}

/// Method information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodInfo {
    /// Method name
    pub name: String,
    /// Return type
    pub return_type: Option<String>,
    /// Parameters
    pub parameters: Vec<ParameterInfo>,
    /// Access modifier
    pub access_modifier: String,
    /// Whether method is abstract
    pub is_abstract: bool,
    /// Whether method is static
    pub is_static: bool,
    /// Whether method is virtual
    pub is_virtual: bool,
    /// Line number
    pub line: usize,
}

/// Field information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldInfo {
    /// Field name
    pub name: String,
    /// Field type
    pub field_type: Option<String>,
    /// Access modifier
    pub access_modifier: String,
    /// Whether field is static
    pub is_static: bool,
    /// Line number
    pub line: usize,
}

/// Property information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyInfo {
    /// Property name
    pub name: String,
    /// Property type
    pub property_type: Option<String>,
    /// Access modifier
    pub access_modifier: String,
    /// Line number
    pub line: usize,
}

/// Parameter information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterInfo {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: Option<String>,
}

/// Component relationship type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentRelationship {
    /// Class inherits from another class
    Inheritance,
    /// Class implements an interface
    Implementation,
    /// Component composes another component (has-a relationship)
    Composition,
    /// Component aggregates another component
    Aggregation,
    /// Component uses another component
    Usage,
    /// Component depends on another component
    Dependency,
}

/// Component relationship information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentRelationshipInfo {
    /// Source component
    pub source: String,
    /// Target component
    pub target: String,
    /// Relationship type
    pub relationship_type: ComponentRelationship,
    /// Strength of relationship (0.0 to 1.0)
    pub strength: f64,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}
