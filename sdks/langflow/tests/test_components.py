"""Tests for LangFlow Nexus components."""

import pytest
from unittest.mock import MagicMock, patch

# Test component imports
def test_import_all_components():
    """Test that all components can be imported."""
    from langflow_nexus import (
        NexusVectorStoreComponent,
        NexusGraphRetrieverComponent,
        NexusGraphMemoryComponent,
        NexusConnectionComponent,
    )

    assert NexusVectorStoreComponent is not None
    assert NexusGraphRetrieverComponent is not None
    assert NexusGraphMemoryComponent is not None
    assert NexusConnectionComponent is not None


def test_import_vectorstore_components():
    """Test vectorstore component imports."""
    from langflow_nexus.vectorstore import (
        NexusVectorStoreComponent,
        NexusVectorSearchComponent,
    )

    assert NexusVectorStoreComponent is not None
    assert NexusVectorSearchComponent is not None


def test_import_retriever_components():
    """Test retriever component imports."""
    from langflow_nexus.retriever import (
        NexusGraphRetrieverComponent,
        NexusHybridSearchComponent,
        NexusGraphTraversalComponent,
    )

    assert NexusGraphRetrieverComponent is not None
    assert NexusHybridSearchComponent is not None
    assert NexusGraphTraversalComponent is not None


def test_import_memory_components():
    """Test memory component imports."""
    from langflow_nexus.memory import (
        NexusGraphMemoryComponent,
        NexusAddMessageComponent,
        NexusSearchMessagesComponent,
        NexusClearMemoryComponent,
    )

    assert NexusGraphMemoryComponent is not None
    assert NexusAddMessageComponent is not None
    assert NexusSearchMessagesComponent is not None
    assert NexusClearMemoryComponent is not None


def test_connection_component_metadata():
    """Test NexusConnectionComponent has correct metadata."""
    from langflow_nexus.connection import NexusConnectionComponent

    assert NexusConnectionComponent.display_name == "Nexus Connection"
    assert NexusConnectionComponent.name == "NexusConnection"
    assert len(NexusConnectionComponent.inputs) >= 4
    assert len(NexusConnectionComponent.outputs) >= 1


def test_vectorstore_component_metadata():
    """Test NexusVectorStoreComponent has correct metadata."""
    from langflow_nexus.vectorstore import NexusVectorStoreComponent

    assert NexusVectorStoreComponent.display_name == "Nexus Vector Store"
    assert NexusVectorStoreComponent.name == "NexusVectorStore"
    assert len(NexusVectorStoreComponent.inputs) >= 4
    assert len(NexusVectorStoreComponent.outputs) >= 1


def test_retriever_component_metadata():
    """Test NexusGraphRetrieverComponent has correct metadata."""
    from langflow_nexus.retriever import NexusGraphRetrieverComponent

    assert NexusGraphRetrieverComponent.display_name == "Nexus Graph Retriever"
    assert NexusGraphRetrieverComponent.name == "NexusGraphRetriever"
    assert len(NexusGraphRetrieverComponent.inputs) >= 6
    assert len(NexusGraphRetrieverComponent.outputs) >= 1


def test_memory_component_metadata():
    """Test NexusGraphMemoryComponent has correct metadata."""
    from langflow_nexus.memory import NexusGraphMemoryComponent

    assert NexusGraphMemoryComponent.display_name == "Nexus Graph Memory"
    assert NexusGraphMemoryComponent.name == "NexusGraphMemory"
    assert len(NexusGraphMemoryComponent.inputs) >= 3
    assert len(NexusGraphMemoryComponent.outputs) >= 1


def test_connection_input_names():
    """Test NexusConnectionComponent has expected inputs."""
    from langflow_nexus.connection import NexusConnectionComponent

    input_names = [inp.name for inp in NexusConnectionComponent.inputs]

    assert "url" in input_names
    assert "api_key" in input_names
    assert "username" in input_names
    assert "password" in input_names


def test_vectorstore_input_names():
    """Test NexusVectorStoreComponent has expected inputs."""
    from langflow_nexus.vectorstore import NexusVectorStoreComponent

    input_names = [inp.name for inp in NexusVectorStoreComponent.inputs]

    assert "client" in input_names
    assert "embedding" in input_names
    assert "label" in input_names


def test_retriever_input_names():
    """Test NexusGraphRetrieverComponent has expected inputs."""
    from langflow_nexus.retriever import NexusGraphRetrieverComponent

    input_names = [inp.name for inp in NexusGraphRetrieverComponent.inputs]

    assert "client" in input_names
    assert "embedding" in input_names
    assert "k" in input_names
    assert "graph_depth" in input_names
    assert "hybrid_search" in input_names


def test_memory_input_names():
    """Test NexusGraphMemoryComponent has expected inputs."""
    from langflow_nexus.memory import NexusGraphMemoryComponent

    input_names = [inp.name for inp in NexusGraphMemoryComponent.inputs]

    assert "client" in input_names
    assert "session_id" in input_names
    assert "user_id" in input_names
    assert "window_size" in input_names


def test_connection_output_names():
    """Test NexusConnectionComponent has expected outputs."""
    from langflow_nexus.connection import NexusConnectionComponent

    output_names = [out.name for out in NexusConnectionComponent.outputs]

    assert "client" in output_names
    assert "status" in output_names


def test_vectorstore_output_names():
    """Test NexusVectorStoreComponent has expected outputs."""
    from langflow_nexus.vectorstore import NexusVectorStoreComponent

    output_names = [out.name for out in NexusVectorStoreComponent.outputs]

    assert "vectorstore" in output_names
    assert "retriever" in output_names


def test_retriever_output_names():
    """Test NexusGraphRetrieverComponent has expected outputs."""
    from langflow_nexus.retriever import NexusGraphRetrieverComponent

    output_names = [out.name for out in NexusGraphRetrieverComponent.outputs]

    assert "retriever" in output_names


def test_memory_output_names():
    """Test NexusGraphMemoryComponent has expected outputs."""
    from langflow_nexus.memory import NexusGraphMemoryComponent

    output_names = [out.name for out in NexusGraphMemoryComponent.outputs]

    assert "memory" in output_names
    assert "messages" in output_names
    assert "summary" in output_names


def test_version():
    """Test package version is defined."""
    from langflow_nexus import __version__

    assert __version__ == "0.1.0"


def test_all_exports():
    """Test __all__ exports are correct."""
    from langflow_nexus import __all__

    expected = [
        "NexusVectorStoreComponent",
        "NexusGraphRetrieverComponent",
        "NexusGraphMemoryComponent",
        "NexusConnectionComponent",
    ]

    for name in expected:
        assert name in __all__
