//! User-Defined Functions (UDF) framework
//!
//! This module provides:
//! - UDF registration and storage
//! - UDF invocation in Cypher expressions
//! - Support for multiple return types
//! - Integration with catalog for persistence

use crate::{Error, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub mod registry;

/// UDF return type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UdfReturnType {
    /// Integer return type
    Integer,
    /// Float return type
    Float,
    /// String return type
    String,
    /// Boolean return type
    Boolean,
    /// Any type (dynamic)
    Any,
    /// List return type
    List(Box<UdfReturnType>),
    /// Map return type
    Map,
}

/// UDF parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdfParameter {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: UdfReturnType,
    /// Whether parameter is required
    pub required: bool,
    /// Default value (if optional)
    pub default: Option<Value>,
}

/// UDF function signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdfSignature {
    /// Function name
    pub name: String,
    /// Function parameters
    pub parameters: Vec<UdfParameter>,
    /// Return type
    pub return_type: UdfReturnType,
    /// Description (optional)
    pub description: Option<String>,
}

/// Trait for UDF implementations
pub trait UdfFunction: Send + Sync {
    /// Get the function signature
    fn signature(&self) -> &UdfSignature;

    /// Execute the UDF with given arguments
    fn execute(&self, args: &[Value]) -> Result<Value>;
}

/// Built-in UDF function wrapper
pub struct BuiltinUdf {
    signature: UdfSignature,
    function: Box<dyn Fn(&[Value]) -> Result<Value> + Send + Sync>,
}

impl BuiltinUdf {
    /// Create a new built-in UDF
    pub fn new<F>(signature: UdfSignature, function: F) -> Self
    where
        F: Fn(&[Value]) -> Result<Value> + Send + Sync + 'static,
    {
        Self {
            signature,
            function: Box::new(function),
        }
    }
}

impl UdfFunction for BuiltinUdf {
    fn signature(&self) -> &UdfSignature {
        &self.signature
    }

    fn execute(&self, args: &[Value]) -> Result<Value> {
        (self.function)(args)
    }
}

/// UDF registry for managing registered functions
#[derive(Clone)]
pub struct UdfRegistry {
    /// Registered UDFs
    udfs: Arc<RwLock<HashMap<String, Arc<dyn UdfFunction>>>>,
}

impl UdfRegistry {
    /// Create a new UDF registry
    pub fn new() -> Self {
        Self {
            udfs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a UDF
    pub fn register(&self, udf: Arc<dyn UdfFunction>) -> Result<()> {
        let name = udf.signature().name.clone();
        let mut udfs = self.udfs.write();

        if udfs.contains_key(&name) {
            return Err(Error::CypherSyntax(format!(
                "UDF '{}' already registered",
                name
            )));
        }

        udfs.insert(name, udf);
        Ok(())
    }

    /// Get a UDF by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn UdfFunction>> {
        let udfs = self.udfs.read();
        udfs.get(name).cloned()
    }

    /// List all registered UDF names
    pub fn list(&self) -> Vec<String> {
        let udfs = self.udfs.read();
        udfs.keys().cloned().collect()
    }

    /// Unregister a UDF
    pub fn unregister(&self, name: &str) -> Result<()> {
        let mut udfs = self.udfs.write();
        udfs.remove(name)
            .ok_or_else(|| Error::CypherSyntax(format!("UDF '{}' not found", name)))?;
        Ok(())
    }

    /// Check if a UDF is registered
    pub fn contains(&self, name: &str) -> bool {
        let udfs = self.udfs.read();
        udfs.contains_key(name)
    }
}

impl Default for UdfRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_udf_registry() {
        let registry = UdfRegistry::new();

        let signature = UdfSignature {
            name: "test_udf".to_string(),
            parameters: vec![],
            return_type: UdfReturnType::Integer,
            description: None,
        };

        let udf = BuiltinUdf::new(signature, |_args| Ok(Value::Number(42.into())));
        registry.register(Arc::new(udf)).unwrap();

        assert!(registry.contains("test_udf"));
        assert_eq!(registry.list(), vec!["test_udf"]);

        let retrieved = registry.get("test_udf").unwrap();
        let result = retrieved.execute(&[]).unwrap();
        assert_eq!(result, Value::Number(42.into()));
    }

    #[test]
    fn test_udf_with_parameters() {
        let registry = UdfRegistry::new();

        let signature = UdfSignature {
            name: "add".to_string(),
            parameters: vec![
                UdfParameter {
                    name: "a".to_string(),
                    param_type: UdfReturnType::Integer,
                    required: true,
                    default: None,
                },
                UdfParameter {
                    name: "b".to_string(),
                    param_type: UdfReturnType::Integer,
                    required: true,
                    default: None,
                },
            ],
            return_type: UdfReturnType::Integer,
            description: Some("Add two integers".to_string()),
        };

        let udf = BuiltinUdf::new(signature, |args| {
            if args.len() != 2 {
                return Err(Error::CypherSyntax("Expected 2 arguments".to_string()));
            }
            let a = args[0]
                .as_i64()
                .ok_or_else(|| Error::CypherSyntax("Invalid argument".to_string()))?;
            let b = args[1]
                .as_i64()
                .ok_or_else(|| Error::CypherSyntax("Invalid argument".to_string()))?;
            Ok(Value::Number((a + b).into()))
        });

        registry.register(Arc::new(udf)).unwrap();

        let retrieved = registry.get("add").unwrap();
        let result = retrieved
            .execute(&[Value::Number(10.into()), Value::Number(20.into())])
            .unwrap();
        assert_eq!(result, Value::Number(30.into()));
    }
}
