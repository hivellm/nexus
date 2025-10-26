//! OpenAPI Documentation Generator for Graph Correlation API

use serde_json::json;

/// Generate OpenAPI 3.0 specification
pub fn generate_openapi_spec() -> serde_json::Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Nexus Graph Correlation API",
            "description": "REST API for graph correlation analysis and pattern detection",
            "version": "1.0.0",
            "contact": {
                "name": "Nexus Team",
                "url": "https://github.com/hivellm/nexus"
            },
            "license": {
                "name": "MIT",
                "url": "https://opensource.org/licenses/MIT"
            }
        },
        "servers": [
            {
                "url": "http://localhost:3000",
                "description": "Development server"
            }
        ],
        "paths": {
            "/graph-correlation/generate": {
                "post": {
                    "summary": "Generate correlation graph",
                    "description": "Create a correlation graph from source code data",
                    "tags": ["Graph Correlation"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/GenerateGraphRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Graph generated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/CorrelationGraph"
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "Invalid request",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Error"
                                    }
                                }
                            }
                        },
                        "429": {
                            "description": "Rate limit exceeded",
                            "headers": {
                                "Retry-After": {
                                    "schema": {
                                        "type": "integer"
                                    },
                                    "description": "Seconds to wait before retry"
                                }
                            }
                        }
                    }
                }
            },
            "/graph-correlation/types": {
                "get": {
                    "summary": "List available graph types",
                    "description": "Get all supported correlation graph types",
                    "tags": ["Graph Correlation"],
                    "responses": {
                        "200": {
                            "description": "List of graph types",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": {
                                            "type": "string",
                                            "enum": ["Call", "Dependency", "DataFlow", "Component"]
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/graph-correlation/auto-generate": {
                "get": {
                    "summary": "Automatically generate graphs from codebase",
                    "description": "Scan project and generate all graph types automatically",
                    "tags": ["Graph Correlation"],
                    "parameters": [
                        {
                            "name": "project_path",
                            "in": "query",
                            "schema": {
                                "type": "string"
                            },
                            "description": "Path to project root"
                        },
                        {
                            "name": "graph_types",
                            "in": "query",
                            "schema": {
                                "type": "string"
                            },
                            "description": "Comma-separated list of graph types"
                        },
                        {
                            "name": "max_files",
                            "in": "query",
                            "schema": {
                                "type": "integer"
                            },
                            "description": "Maximum files to analyze"
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Graphs generated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/AutoGenerateResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/health": {
                "get": {
                    "summary": "Health check",
                    "description": "Check if the API is running",
                    "tags": ["System"],
                    "responses": {
                        "200": {
                            "description": "API is healthy",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "status": {
                                                "type": "string",
                                                "example": "ok"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "GenerateGraphRequest": {
                    "type": "object",
                    "required": ["graph_type", "files", "functions", "imports"],
                    "properties": {
                        "graph_type": {
                            "type": "string",
                            "enum": ["Call", "Dependency", "DataFlow", "Component"],
                            "description": "Type of graph to generate"
                        },
                        "files": {
                            "type": "object",
                            "additionalProperties": {
                                "type": "string"
                            },
                            "description": "Map of file paths to contents"
                        },
                        "functions": {
                            "type": "object",
                            "additionalProperties": {
                                "type": "string"
                            },
                            "description": "Map of function names to signatures"
                        },
                        "imports": {
                            "type": "object",
                            "additionalProperties": {
                                "type": "string"
                            },
                            "description": "Map of files to import statements"
                        }
                    }
                },
                "CorrelationGraph": {
                    "type": "object",
                    "properties": {
                        "graph_type": {
                            "type": "string",
                            "enum": ["Call", "Dependency", "DataFlow", "Component"]
                        },
                        "nodes": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/GraphNode"
                            }
                        },
                        "edges": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/GraphEdge"
                            }
                        },
                        "metadata": {
                            "type": "object",
                            "additionalProperties": {
                                "type": "string"
                            }
                        }
                    }
                },
                "GraphNode": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string"
                        },
                        "label": {
                            "type": "string"
                        },
                        "node_type": {
                            "type": "string",
                            "enum": ["Function", "Module", "Class", "File"]
                        },
                        "properties": {
                            "type": "object",
                            "additionalProperties": {
                                "type": "string"
                            }
                        }
                    }
                },
                "GraphEdge": {
                    "type": "object",
                    "properties": {
                        "source": {
                            "type": "string"
                        },
                        "target": {
                            "type": "string"
                        },
                        "edge_type": {
                            "type": "string",
                            "enum": ["Calls", "DependsOn", "DataFlow", "Contains"]
                        },
                        "properties": {
                            "type": "object",
                            "additionalProperties": {
                                "type": "string"
                            }
                        }
                    }
                },
                "AutoGenerateResponse": {
                    "type": "object",
                    "properties": {
                        "files_analyzed": {
                            "type": "integer"
                        },
                        "graphs": {
                            "type": "object",
                            "additionalProperties": {
                                "$ref": "#/components/schemas/GraphSummary"
                            }
                        },
                        "extraction_time_ms": {
                            "type": "integer"
                        },
                        "generation_time_ms": {
                            "type": "integer"
                        }
                    }
                },
                "GraphSummary": {
                    "type": "object",
                    "properties": {
                        "graph_type": {
                            "type": "string"
                        },
                        "node_count": {
                            "type": "integer"
                        },
                        "edge_count": {
                            "type": "integer"
                        }
                    }
                },
                "Error": {
                    "type": "object",
                    "properties": {
                        "error": {
                            "type": "string",
                            "description": "Error message"
                        }
                    }
                }
            },
            "securitySchemes": {
                "ApiKeyAuth": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "X-API-Key"
                }
            }
        },
        "tags": [
            {
                "name": "Graph Correlation",
                "description": "Graph correlation analysis and generation"
            },
            {
                "name": "System",
                "description": "System health and status"
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_openapi_spec() {
        let spec = generate_openapi_spec();
        
        assert_eq!(spec["openapi"], "3.0.3");
        assert_eq!(spec["info"]["title"], "Nexus Graph Correlation API");
        assert!(spec["paths"].is_object());
        assert!(spec["components"]["schemas"].is_object());
    }

    #[test]
    fn test_openapi_has_required_paths() {
        let spec = generate_openapi_spec();
        
        assert!(spec["paths"]["/graph-correlation/generate"].is_object());
        assert!(spec["paths"]["/graph-correlation/types"].is_object());
        assert!(spec["paths"]["/graph-correlation/auto-generate"].is_object());
        assert!(spec["paths"]["/health"].is_object());
    }

    #[test]
    fn test_openapi_has_schemas() {
        let spec = generate_openapi_spec();
        let schemas = &spec["components"]["schemas"];
        
        assert!(schemas["GenerateGraphRequest"].is_object());
        assert!(schemas["CorrelationGraph"].is_object());
        assert!(schemas["GraphNode"].is_object());
        assert!(schemas["GraphEdge"].is_object());
        assert!(schemas["AutoGenerateResponse"].is_object());
        assert!(schemas["Error"].is_object());
    }
}

