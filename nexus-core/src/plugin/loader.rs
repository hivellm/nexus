//! Plugin loader for dynamic plugin loading

use crate::plugin::{Plugin, PluginResult};
use std::path::Path;
use std::sync::Arc;

/// Plugin loader for loading plugins from files
///
/// Currently, this is a placeholder for future dynamic loading support.
/// For now, plugins must be statically linked.
pub struct PluginLoader;

impl PluginLoader {
    /// Create a new plugin loader
    pub fn new() -> Self {
        Self
    }

    /// Load a plugin from a file path
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin cannot be loaded.
    ///
    /// # Note
    ///
    /// Dynamic loading is not yet implemented. Plugins must be statically linked.
    pub fn load_from_path<P: AsRef<Path>>(&self, _path: P) -> PluginResult<Arc<dyn Plugin>> {
        Err(crate::Error::Plugin(
            "Dynamic plugin loading is not yet implemented. Plugins must be statically linked."
                .to_string(),
        ))
    }

    /// Load plugins from a directory
    ///
    /// # Errors
    ///
    /// Returns an error if any plugin cannot be loaded.
    ///
    /// # Note
    ///
    /// Dynamic loading is not yet implemented. Plugins must be statically linked.
    pub fn load_from_directory<P: AsRef<Path>>(
        &self,
        _directory: P,
    ) -> PluginResult<Vec<Arc<dyn Plugin>>> {
        Err(crate::Error::Plugin(
            "Dynamic plugin loading is not yet implemented. Plugins must be statically linked."
                .to_string(),
        ))
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}
