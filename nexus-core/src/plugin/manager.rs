//! Plugin manager for loading and managing plugins

use crate::Result as PluginResult;
use crate::catalog::Catalog;
use crate::graph::procedures::ProcedureRegistry;
use crate::plugin::context::PluginContext;
use crate::plugin::{Plugin, PluginMetadata};
use crate::udf::UdfRegistry;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Plugin manager for loading and managing plugins
#[derive(Clone)]
pub struct PluginManager {
    /// Loaded plugins
    plugins: Arc<RwLock<HashMap<String, Arc<dyn Plugin>>>>,
    /// Plugin metadata
    metadata: Arc<RwLock<HashMap<String, PluginMetadata>>>,
    /// UDF registry
    udf_registry: Option<Arc<UdfRegistry>>,
    /// Procedure registry
    procedure_registry: Option<Arc<ProcedureRegistry>>,
    /// Catalog
    catalog: Option<Arc<Catalog>>,
}

impl PluginManager {
    /// Create a new plugin manager
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            udf_registry: None,
            procedure_registry: None,
            catalog: None,
        }
    }

    /// Create a plugin manager with registries
    pub fn with_registries(
        udf_registry: Option<Arc<UdfRegistry>>,
        procedure_registry: Option<Arc<ProcedureRegistry>>,
        catalog: Option<Arc<Catalog>>,
    ) -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            udf_registry,
            procedure_registry,
            catalog,
        }
    }

    /// Load a plugin
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin fails to initialize or if a plugin with the same name is already loaded.
    pub fn load_plugin(&self, plugin: Arc<dyn Plugin>) -> PluginResult<()> {
        let name = plugin.name().to_string();
        let version = plugin.version().to_string();

        // Check if plugin is already loaded
        {
            let plugins = self.plugins.read();
            if plugins.contains_key(&name) {
                return Err(crate::Error::Plugin(format!(
                    "Plugin '{}' is already loaded",
                    name
                )));
            }
        }

        // Create plugin context
        let mut ctx = PluginContext::new(
            self.udf_registry.clone(),
            self.procedure_registry.clone(),
            self.catalog.clone(),
        );

        // Initialize plugin
        plugin.initialize(&mut ctx)?;

        // Store plugin
        {
            let mut plugins = self.plugins.write();
            plugins.insert(name.clone(), plugin);
        }

        // Store metadata
        {
            let mut metadata = self.metadata.write();
            metadata.insert(name.clone(), PluginMetadata::new(name, version));
        }

        Ok(())
    }

    /// Unload a plugin
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin fails to shutdown or if the plugin is not loaded.
    pub fn unload_plugin(&self, name: &str) -> PluginResult<()> {
        // Get plugin
        let plugin = {
            let mut plugins = self.plugins.write();
            plugins
                .remove(name)
                .ok_or_else(|| crate::Error::Plugin(format!("Plugin '{}' not found", name)))?
        };

        // Shutdown plugin
        plugin.shutdown()?;

        // Remove metadata
        {
            let mut metadata = self.metadata.write();
            metadata.remove(name);
        }

        Ok(())
    }

    /// Get a plugin by name
    pub fn get_plugin(&self, name: &str) -> Option<Arc<dyn Plugin>> {
        let plugins = self.plugins.read();
        plugins.get(name).cloned()
    }

    /// List all loaded plugins
    pub fn list_plugins(&self) -> Vec<String> {
        let plugins = self.plugins.read();
        plugins.keys().cloned().collect()
    }

    /// Get plugin metadata
    pub fn get_metadata(&self, name: &str) -> Option<PluginMetadata> {
        let metadata = self.metadata.read();
        metadata.get(name).cloned()
    }

    /// Check if a plugin is loaded
    pub fn is_loaded(&self, name: &str) -> bool {
        let plugins = self.plugins.read();
        plugins.contains_key(name)
    }

    /// Shutdown all plugins
    ///
    /// # Errors
    ///
    /// Returns an error if any plugin fails to shutdown.
    pub fn shutdown_all(&self) -> PluginResult<()> {
        let plugins = {
            let mut plugins = self.plugins.write();
            let plugins_vec: Vec<_> = plugins.drain().collect();
            plugins_vec
        };

        for (name, plugin) in plugins {
            if let Err(e) = plugin.shutdown() {
                return Err(crate::Error::Plugin(format!(
                    "Failed to shutdown plugin '{}': {}",
                    name, e
                )));
            }
        }

        Ok(())
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}
