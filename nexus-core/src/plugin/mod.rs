//! Plugin system for extending Nexus functionality
//!
//! This module provides a plugin architecture that allows extending Nexus with:
//! - Custom UDFs (User-Defined Functions)
//! - Custom Procedures
//! - Custom indexes
//! - Custom validators
//!
//! # Architecture
//!
//! Plugins are loaded dynamically and can register extensions at initialization time.
//! The plugin system manages the lifecycle of plugins and provides a safe API for
//! registering extensions.
//!
//! # Example
//!
//! ```rust,no_run
//! use nexus_core::plugin::{Plugin, PluginContext, PluginResult};
//! use nexus_core::udf::{BuiltinUdf, UdfSignature, UdfReturnType};
//! use std::sync::Arc;
//!
//! #[derive(Debug)]
//! pub struct MyPlugin;
//!
//! impl Plugin for MyPlugin {
//!     fn name(&self) -> &str {
//!         "my_plugin"
//!     }
//!
//!     fn version(&self) -> &str {
//!         "1.0.0"
//!     }
//!
//!     fn initialize(&self, ctx: &mut PluginContext) -> PluginResult<()> {
//!         // Register a custom UDF
//!         let udf = BuiltinUdf::new(
//!             UdfSignature {
//!                 name: "my_function".to_string(),
//!                 parameters: vec![],
//!                 return_type: UdfReturnType::Integer,
//!                 description: None,
//!             },
//!             |_args| Ok(serde_json::Value::Number(42.into())),
//!         );
//!         ctx.register_udf(Arc::new(udf))?;
//!         Ok(())
//!     }
//!
//!     fn shutdown(&self) -> PluginResult<()> {
//!         Ok(())
//!     }
//! }
//! ```

pub mod context;
pub mod loader;
pub mod manager;

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests;

pub use context::{PluginContext, PluginResult};
pub use loader::PluginLoader;
pub use manager::PluginManager;

use std::fmt::Debug;

/// Plugin trait that all plugins must implement
pub trait Plugin: Send + Sync + Debug {
    /// Get the plugin name
    fn name(&self) -> &str;

    /// Get the plugin version
    fn version(&self) -> &str;

    /// Initialize the plugin
    ///
    /// This is called when the plugin is loaded. Plugins should register
    /// their extensions (UDFs, procedures, etc.) here.
    fn initialize(&self, ctx: &mut PluginContext) -> PluginResult<()>;

    /// Shutdown the plugin
    ///
    /// This is called when the plugin is being unloaded. Plugins should
    /// clean up any resources here.
    fn shutdown(&self) -> PluginResult<()>;
}

/// Plugin metadata
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    /// Plugin name
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    pub description: Option<String>,
    /// Plugin author
    pub author: Option<String>,
    /// Plugin dependencies
    pub dependencies: Vec<String>,
}

impl PluginMetadata {
    /// Create new plugin metadata
    pub fn new(name: String, version: String) -> Self {
        Self {
            name,
            version,
            description: None,
            author: None,
            dependencies: Vec::new(),
        }
    }
}
