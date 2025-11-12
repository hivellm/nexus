//! UDF Registry for managing user-defined functions
//!
//! This module provides persistent storage of UDFs in the catalog

use super::UdfFunction;
use crate::catalog::Catalog;
use crate::{Error, Result};
use parking_lot::RwLock;
use std::sync::Arc;

/// UDF storage key prefix in catalog
const UDF_STORAGE_PREFIX: &str = "__udf__";

/// UDF registry with catalog persistence
pub struct PersistentUdfRegistry {
    /// Catalog for storage
    catalog: Arc<Catalog>,
    /// In-memory cache of UDFs
    cache: Arc<RwLock<std::collections::HashMap<String, Arc<dyn UdfFunction>>>>,
}

impl PersistentUdfRegistry {
    /// Create a new persistent UDF registry
    pub fn new(catalog: Arc<Catalog>) -> Self {
        let registry = Self {
            catalog: catalog.clone(),
            cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
        };

        // Load UDFs from catalog on initialization
        // Note: We can only load signatures, not the actual function implementations
        // Custom UDF implementations would need to be re-registered
        if let Ok(_udf_names) = catalog.list_udfs() {
            // Signatures are loaded but function implementations need to be provided
            // This is expected - UDFs with custom implementations need to be re-registered
        }

        registry
    }

    /// Register a UDF and persist to catalog
    pub fn register(&self, udf: Arc<dyn UdfFunction>) -> Result<()> {
        let name = udf.signature().name.clone();

        // Check if already registered
        {
            let cache = self.cache.read();
            if cache.contains_key(&name) {
                return Err(Error::CypherSyntax(format!(
                    "UDF '{}' already registered",
                    name
                )));
            }
        }

        // Persist signature to catalog
        let signature = udf.signature().clone();
        self.catalog.store_udf(&signature)?;

        // Cache the UDF instance
        {
            let mut cache = self.cache.write();
            cache.insert(name, udf);
        }

        Ok(())
    }

    /// Get a UDF by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn UdfFunction>> {
        let cache = self.cache.read();
        cache.get(name).cloned()
    }

    /// List all registered UDF names
    pub fn list(&self) -> Vec<String> {
        let cache = self.cache.read();
        cache.keys().cloned().collect()
    }

    /// Unregister a UDF
    pub fn unregister(&self, name: &str) -> Result<()> {
        // Remove from cache
        {
            let mut cache = self.cache.write();
            cache
                .remove(name)
                .ok_or_else(|| Error::CypherSyntax(format!("UDF '{}' not found", name)))?;
        }

        // Remove from catalog
        self.catalog.remove_udf(name)?;

        Ok(())
    }

    /// Check if a UDF is registered
    pub fn contains(&self, name: &str) -> bool {
        let cache = self.cache.read();
        cache.contains_key(name)
    }

    /// Load UDF signatures from catalog
    pub fn load_signatures_from_catalog(&self) -> Result<Vec<super::UdfSignature>> {
        let names = self.catalog.list_udfs()?;
        let mut signatures = Vec::new();
        for name in names {
            if let Some(sig) = self.catalog.get_udf(&name)? {
                signatures.push(sig);
            }
        }
        Ok(signatures)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::udf::{BuiltinUdf, UdfParameter, UdfReturnType, UdfSignature};
    use serde_json::Value;
    use tempfile::TempDir;

    fn create_test_catalog() -> (Arc<Catalog>, TempDir) {
        let dir = TempDir::new().unwrap();
        let catalog = Catalog::new(dir.path()).unwrap();
        (Arc::new(catalog), dir)
    }

    #[test]
    fn test_persistent_udf_registry() {
        let (catalog, _dir) = create_test_catalog();
        let registry = PersistentUdfRegistry::new(catalog.clone());

        let signature = UdfSignature {
            name: "test_udf".to_string(),
            parameters: vec![],
            return_type: UdfReturnType::Integer,
            description: None,
        };

        let udf = BuiltinUdf::new(signature.clone(), |_args| Ok(Value::Number(42.into())));
        registry.register(Arc::new(udf)).unwrap();

        // Verify it's in cache
        assert!(registry.contains("test_udf"));
        assert_eq!(registry.list(), vec!["test_udf"]);

        // Verify it's persisted in catalog
        let catalog_sig = catalog.get_udf("test_udf").unwrap();
        assert!(catalog_sig.is_some());
        assert_eq!(catalog_sig.unwrap().name, "test_udf");

        // Test execution
        let retrieved = registry.get("test_udf").unwrap();
        let result = retrieved.execute(&[]).unwrap();
        assert_eq!(result, Value::Number(42.into()));

        // Test unregister
        registry.unregister("test_udf").unwrap();
        assert!(!registry.contains("test_udf"));
        let catalog_sig_after = catalog.get_udf("test_udf").unwrap();
        assert!(catalog_sig_after.is_none());
    }

    #[test]
    fn test_persistent_udf_registry_with_parameters() {
        let (catalog, _dir) = create_test_catalog();
        let registry = PersistentUdfRegistry::new(catalog.clone());

        let signature = UdfSignature {
            name: "multiply".to_string(),
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
            description: Some("Multiply two integers".to_string()),
        };

        let udf = BuiltinUdf::new(signature.clone(), |args| {
            if args.len() != 2 {
                return Err(Error::CypherSyntax("Expected 2 arguments".to_string()));
            }
            let a = args[0]
                .as_i64()
                .ok_or_else(|| Error::CypherSyntax("Invalid argument".to_string()))?;
            let b = args[1]
                .as_i64()
                .ok_or_else(|| Error::CypherSyntax("Invalid argument".to_string()))?;
            Ok(Value::Number((a * b).into()))
        });

        registry.register(Arc::new(udf)).unwrap();

        // Verify persistence
        let catalog_sig = catalog.get_udf("multiply").unwrap();
        assert!(catalog_sig.is_some());
        let sig = catalog_sig.unwrap();
        assert_eq!(sig.parameters.len(), 2);

        // Test execution
        let retrieved = registry.get("multiply").unwrap();
        let result = retrieved
            .execute(&[Value::Number(5.into()), Value::Number(7.into())])
            .unwrap();
        assert_eq!(result, Value::Number(35.into()));
    }

    #[test]
    fn test_load_signatures_from_catalog() {
        let (catalog, _dir) = create_test_catalog();

        // Store signatures directly in catalog
        let sig1 = UdfSignature {
            name: "udf1".to_string(),
            parameters: vec![],
            return_type: UdfReturnType::String,
            description: None,
        };
        let sig2 = UdfSignature {
            name: "udf2".to_string(),
            parameters: vec![],
            return_type: UdfReturnType::Float,
            description: None,
        };

        catalog.store_udf(&sig1).unwrap();
        catalog.store_udf(&sig2).unwrap();

        // Create registry and load signatures
        let registry = PersistentUdfRegistry::new(catalog.clone());
        let signatures = registry.load_signatures_from_catalog().unwrap();

        assert_eq!(signatures.len(), 2);
        let names: Vec<String> = signatures.iter().map(|s| s.name.clone()).collect();
        assert!(names.contains(&"udf1".to_string()));
        assert!(names.contains(&"udf2".to_string()));
    }

    #[test]
    fn test_duplicate_registration() {
        let (catalog, _dir) = create_test_catalog();
        let registry = PersistentUdfRegistry::new(catalog.clone());

        let signature = UdfSignature {
            name: "duplicate_test".to_string(),
            parameters: vec![],
            return_type: UdfReturnType::Integer,
            description: None,
        };

        let udf1 = BuiltinUdf::new(signature.clone(), |_args| Ok(Value::Number(1.into())));
        registry.register(Arc::new(udf1)).unwrap();

        let udf2 = BuiltinUdf::new(signature, |_args| Ok(Value::Number(2.into())));
        let result = registry.register(Arc::new(udf2));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("already registered")
        );
    }
}
