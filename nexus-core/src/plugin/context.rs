//! Plugin context for registering extensions

use crate::Error;
use crate::catalog::Catalog;
use crate::graph::procedures::{CustomProcedure, ProcedureRegistry};
use crate::udf::{UdfFunction, UdfRegistry};
use std::sync::Arc;

/// Result type for plugin operations
pub type PluginResult<T> = crate::Result<T>;

/// Plugin context provided to plugins during initialization
///
/// This context allows plugins to register UDFs, procedures, and other extensions.
pub struct PluginContext {
    /// UDF registry for registering UDFs
    udf_registry: Option<Arc<UdfRegistry>>,
    /// Procedure registry for registering procedures
    procedure_registry: Option<Arc<ProcedureRegistry>>,
    /// Catalog for persistence
    catalog: Option<Arc<Catalog>>,
}

impl PluginContext {
    /// Create a new plugin context
    pub fn new(
        udf_registry: Option<Arc<UdfRegistry>>,
        procedure_registry: Option<Arc<ProcedureRegistry>>,
        catalog: Option<Arc<Catalog>>,
    ) -> Self {
        Self {
            udf_registry,
            procedure_registry,
            catalog,
        }
    }

    /// Register a UDF
    ///
    /// # Errors
    ///
    /// Returns an error if UDF registration fails or if UDF registry is not available.
    pub fn register_udf(&mut self, udf: Arc<dyn UdfFunction>) -> PluginResult<()> {
        let registry = self
            .udf_registry
            .as_ref()
            .ok_or_else(|| Error::Plugin("UDF registry not available".to_string()))?;
        registry.register(udf)?;
        Ok(())
    }

    /// Register a custom procedure
    ///
    /// # Errors
    ///
    /// Returns an error if procedure registration fails or if procedure registry is not available.
    pub fn register_procedure(&mut self, procedure: CustomProcedure) -> PluginResult<()> {
        let registry = self
            .procedure_registry
            .as_ref()
            .ok_or_else(|| Error::Plugin("Procedure registry not available".to_string()))?;
        registry.register_custom(procedure)?;
        Ok(())
    }

    /// Get the catalog (if available)
    pub fn catalog(&self) -> Option<&Arc<Catalog>> {
        self.catalog.as_ref()
    }

    /// Get the UDF registry (if available)
    pub fn udf_registry(&self) -> Option<&Arc<UdfRegistry>> {
        self.udf_registry.as_ref()
    }

    /// Get the procedure registry (if available)
    pub fn procedure_registry(&self) -> Option<&Arc<ProcedureRegistry>> {
        self.procedure_registry.as_ref()
    }
}
