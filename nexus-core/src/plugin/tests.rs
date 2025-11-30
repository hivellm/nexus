//! Tests for plugin system

#[cfg(test)]
mod plugin_tests {
    use crate::plugin::{Plugin, PluginContext, PluginManager, PluginResult};
    use crate::udf::{BuiltinUdf, UdfRegistry, UdfReturnType, UdfSignature};
    use serde_json::Value;
    use std::sync::Arc;

    /// Test plugin that registers a UDF
    #[derive(Debug)]
    struct TestUdfPlugin;

    impl Plugin for TestUdfPlugin {
        fn name(&self) -> &str {
            "test_udf_plugin"
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn initialize(&self, ctx: &mut PluginContext) -> PluginResult<()> {
            let signature = UdfSignature {
                name: "plugin_udf".to_string(),
                parameters: vec![],
                return_type: UdfReturnType::Integer,
                description: Some("Test UDF from plugin".to_string()),
            };

            let udf = BuiltinUdf::new(signature, |_args| Ok(Value::Number(100.into())));
            ctx.register_udf(Arc::new(udf))?;
            Ok(())
        }

        fn shutdown(&self) -> PluginResult<()> {
            Ok(())
        }
    }

    /// Test plugin that registers a procedure
    #[derive(Debug)]
    struct TestProcedurePlugin;

    impl Plugin for TestProcedurePlugin {
        fn name(&self) -> &str {
            "test_procedure_plugin"
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn initialize(&self, ctx: &mut PluginContext) -> PluginResult<()> {
            use crate::graph::procedures::{CustomProcedure, ParameterType, ProcedureParameter};

            let procedure = CustomProcedure::new(
                "plugin.procedure".to_string(),
                vec![ProcedureParameter {
                    name: "value".to_string(),
                    param_type: ParameterType::Integer,
                    required: true,
                    default: None,
                }],
                |_graph, args| {
                    let value = args.get("value").and_then(|v| v.as_i64()).unwrap_or(0);
                    Ok(crate::graph::procedures::ProcedureResult {
                        columns: vec!["result".to_string()],
                        rows: vec![vec![Value::Number((value * 2).into())]],
                    })
                },
            );
            ctx.register_procedure(procedure)?;
            Ok(())
        }

        fn shutdown(&self) -> PluginResult<()> {
            Ok(())
        }
    }

    #[test]
    fn test_plugin_manager_creation() {
        let manager = PluginManager::new();
        assert_eq!(manager.list_plugins().len(), 0);
    }

    #[test]
    fn test_plugin_manager_with_registries() {
        let udf_registry = Arc::new(UdfRegistry::new());
        let manager = PluginManager::with_registries(Some(udf_registry.clone()), None, None);
        assert_eq!(manager.list_plugins().len(), 0);
    }

    #[test]
    fn test_load_plugin() {
        let udf_registry = Arc::new(UdfRegistry::new());
        let manager = PluginManager::with_registries(Some(udf_registry.clone()), None, None);

        let plugin = Arc::new(TestUdfPlugin);
        manager.load_plugin(plugin).unwrap();

        assert_eq!(manager.list_plugins().len(), 1);
        assert!(manager.is_loaded("test_udf_plugin"));
        assert!(udf_registry.contains("plugin_udf"));

        // Verify UDF works
        let udf = udf_registry.get("plugin_udf").unwrap();
        let result = udf.execute(&[]).unwrap();
        assert_eq!(result, Value::Number(100.into()));
    }

    #[test]
    fn test_load_multiple_plugins() {
        let udf_registry = Arc::new(UdfRegistry::new());
        let procedure_registry = Arc::new(crate::graph::procedures::ProcedureRegistry::new());
        let manager = PluginManager::with_registries(
            Some(udf_registry.clone()),
            Some(procedure_registry.clone()),
            None,
        );

        let plugin1 = Arc::new(TestUdfPlugin);
        let plugin2 = Arc::new(TestProcedurePlugin);

        manager.load_plugin(plugin1).unwrap();
        manager.load_plugin(plugin2).unwrap();

        assert_eq!(manager.list_plugins().len(), 2);
        assert!(manager.is_loaded("test_udf_plugin"));
        assert!(manager.is_loaded("test_procedure_plugin"));
        assert!(udf_registry.contains("plugin_udf"));
        assert!(procedure_registry.contains("plugin.procedure"));
    }

    #[test]
    fn test_duplicate_plugin_loading() {
        let udf_registry = Arc::new(UdfRegistry::new());
        let manager = PluginManager::with_registries(Some(udf_registry), None, None);
        let plugin = Arc::new(TestUdfPlugin);

        manager.load_plugin(plugin.clone()).unwrap();
        let result = manager.load_plugin(plugin);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already loaded"));
    }

    #[test]
    fn test_unload_plugin() {
        let udf_registry = Arc::new(UdfRegistry::new());
        let manager = PluginManager::with_registries(Some(udf_registry.clone()), None, None);

        let plugin = Arc::new(TestUdfPlugin);
        manager.load_plugin(plugin).unwrap();
        assert_eq!(manager.list_plugins().len(), 1);

        manager.unload_plugin("test_udf_plugin").unwrap();
        assert_eq!(manager.list_plugins().len(), 0);
        assert!(!manager.is_loaded("test_udf_plugin"));
    }

    #[test]
    fn test_unload_nonexistent_plugin() {
        let manager = PluginManager::new();
        let result = manager.unload_plugin("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_get_plugin() {
        let udf_registry = Arc::new(UdfRegistry::new());
        let manager = PluginManager::with_registries(Some(udf_registry), None, None);
        let plugin = Arc::new(TestUdfPlugin);

        manager.load_plugin(plugin.clone()).unwrap();

        let retrieved = manager.get_plugin("test_udf_plugin");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "test_udf_plugin");
    }

    #[test]
    fn test_get_plugin_metadata() {
        let udf_registry = Arc::new(UdfRegistry::new());
        let manager = PluginManager::with_registries(Some(udf_registry), None, None);
        let plugin = Arc::new(TestUdfPlugin);

        manager.load_plugin(plugin).unwrap();

        let metadata = manager.get_metadata("test_udf_plugin");
        assert!(metadata.is_some());
        let meta = metadata.unwrap();
        assert_eq!(meta.name, "test_udf_plugin");
        assert_eq!(meta.version, "1.0.0");
    }

    #[test]
    fn test_shutdown_all_plugins() {
        let udf_registry = Arc::new(UdfRegistry::new());
        let procedure_registry = Arc::new(crate::graph::procedures::ProcedureRegistry::new());
        let manager =
            PluginManager::with_registries(Some(udf_registry), Some(procedure_registry), None);
        let plugin1 = Arc::new(TestUdfPlugin);
        let plugin2 = Arc::new(TestProcedurePlugin);

        manager.load_plugin(plugin1).unwrap();
        manager.load_plugin(plugin2).unwrap();
        assert_eq!(manager.list_plugins().len(), 2);

        manager.shutdown_all().unwrap();
        assert_eq!(manager.list_plugins().len(), 0);
    }

    #[test]
    fn test_plugin_context_without_registries() {
        let mut ctx = PluginContext::new(None, None, None);

        let signature = UdfSignature {
            name: "test".to_string(),
            parameters: vec![],
            return_type: UdfReturnType::Integer,
            description: None,
        };
        let udf = BuiltinUdf::new(signature, |_args| Ok(Value::Number(1.into())));

        let result = ctx.register_udf(Arc::new(udf));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("UDF registry not available")
        );
    }

    #[test]
    fn test_plugin_context_with_udf_registry_only() {
        let udf_registry = Arc::new(UdfRegistry::new());
        let mut ctx = PluginContext::new(Some(udf_registry.clone()), None, None);

        let signature = UdfSignature {
            name: "context_udf".to_string(),
            parameters: vec![],
            return_type: UdfReturnType::Integer,
            description: None,
        };
        let udf = BuiltinUdf::new(signature, |_args| Ok(Value::Number(42.into())));

        ctx.register_udf(Arc::new(udf)).unwrap();
        assert!(udf_registry.contains("context_udf"));
    }

    #[test]
    fn test_plugin_context_with_procedure_registry_only() {
        let procedure_registry = Arc::new(crate::graph::procedures::ProcedureRegistry::new());
        let mut ctx = PluginContext::new(None, Some(procedure_registry.clone()), None);

        use crate::graph::procedures::CustomProcedure;
        let procedure =
            CustomProcedure::new("context.procedure".to_string(), vec![], |_graph, _args| {
                Ok(crate::graph::procedures::ProcedureResult {
                    columns: vec!["result".to_string()],
                    rows: vec![vec![Value::String("test".to_string())]],
                })
            });

        ctx.register_procedure(procedure).unwrap();
        assert!(procedure_registry.contains("context.procedure"));
    }

    #[test]
    fn test_plugin_context_with_all_registries() {
        let udf_registry = Arc::new(UdfRegistry::new());
        let procedure_registry = Arc::new(crate::graph::procedures::ProcedureRegistry::new());
        let test_ctx = crate::testing::TestContext::new();
        let catalog = Arc::new(crate::catalog::Catalog::new(test_ctx.path()).unwrap());
        let mut ctx = PluginContext::new(
            Some(udf_registry.clone()),
            Some(procedure_registry.clone()),
            Some(catalog.clone()),
        );

        // Register UDF
        let udf_sig = UdfSignature {
            name: "all_registries_udf".to_string(),
            parameters: vec![],
            return_type: UdfReturnType::String,
            description: None,
        };
        let udf = BuiltinUdf::new(udf_sig, |_args| Ok(Value::String("udf_result".to_string())));
        ctx.register_udf(Arc::new(udf)).unwrap();

        // Register procedure
        use crate::graph::procedures::CustomProcedure;
        let procedure = CustomProcedure::new(
            "all_registries.procedure".to_string(),
            vec![],
            |_graph, _args| {
                Ok(crate::graph::procedures::ProcedureResult {
                    columns: vec!["result".to_string()],
                    rows: vec![vec![Value::String("proc_result".to_string())]],
                })
            },
        );
        ctx.register_procedure(procedure).unwrap();

        // Verify both are registered
        assert!(udf_registry.contains("all_registries_udf"));
        assert!(procedure_registry.contains("all_registries.procedure"));
    }

    #[test]
    fn test_plugin_context_procedure_registry_not_available() {
        let mut ctx = PluginContext::new(None, None, None);

        use crate::graph::procedures::CustomProcedure;
        let procedure =
            CustomProcedure::new("test.procedure".to_string(), vec![], |_graph, _args| {
                Ok(crate::graph::procedures::ProcedureResult {
                    columns: vec!["result".to_string()],
                    rows: vec![vec![Value::String("test".to_string())]],
                })
            });

        let result = ctx.register_procedure(procedure);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Procedure registry not available")
        );
    }

    #[test]
    fn test_plugin_manager_list_plugins() {
        let manager = PluginManager::new();
        assert_eq!(manager.list_plugins().len(), 0);

        let udf_registry = Arc::new(UdfRegistry::new());
        let manager = PluginManager::with_registries(Some(udf_registry), None, None);
        let plugin = Arc::new(TestUdfPlugin);
        manager.load_plugin(plugin).unwrap();

        let plugins = manager.list_plugins();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0], "test_udf_plugin");
    }

    #[test]
    fn test_plugin_manager_get_nonexistent_plugin() {
        let manager = PluginManager::new();
        assert!(manager.get_plugin("nonexistent").is_none());
    }

    #[test]
    fn test_plugin_manager_get_metadata_nonexistent() {
        let manager = PluginManager::new();
        assert!(manager.get_metadata("nonexistent").is_none());
    }

    #[test]
    fn test_plugin_shutdown_individual() {
        let udf_registry = Arc::new(UdfRegistry::new());
        let manager = PluginManager::with_registries(Some(udf_registry), None, None);
        let plugin = Arc::new(TestUdfPlugin);

        manager.load_plugin(plugin).unwrap();
        assert_eq!(manager.list_plugins().len(), 1);

        // Shutdown should call plugin.shutdown()
        manager.shutdown_all().unwrap();
        assert_eq!(manager.list_plugins().len(), 0);
    }
}
