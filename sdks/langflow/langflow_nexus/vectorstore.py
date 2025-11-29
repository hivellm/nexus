"""Nexus Vector Store Component for LangFlow."""

from typing import Any, List, Optional

from langflow.custom import Component
from langflow.io import (
    HandleInput,
    IntInput,
    MessageTextInput,
    Output,
    DataInput,
)
from langflow.schema import Data
from langchain_core.documents import Document
from langchain_core.embeddings import Embeddings

from langchain_nexus import NexusVectorStore, NexusClient


class NexusVectorStoreComponent(Component):
    """LangFlow component for Nexus Vector Store.

    This component provides vector storage and similarity search
    capabilities using Nexus graph database.

    Inputs:
        client: NexusClient from NexusConnection component
        embedding: Embeddings model (e.g., OpenAI, HuggingFace)
        label: Node label for document storage
        documents: Optional documents to add
        k: Number of results for similarity search

    Outputs:
        vectorstore: NexusVectorStore instance
        retriever: Retriever for use in chains
    """

    display_name = "Nexus Vector Store"
    description = "Store and search documents using vector embeddings in Nexus"
    icon = "database"
    name = "NexusVectorStore"

    inputs = [
        HandleInput(
            name="client",
            display_name="Nexus Client",
            info="NexusClient from NexusConnection component",
            input_types=["NexusClient"],
            required=True,
        ),
        HandleInput(
            name="embedding",
            display_name="Embedding Model",
            info="Embeddings model for generating vectors",
            input_types=["Embeddings"],
            required=True,
        ),
        MessageTextInput(
            name="label",
            display_name="Node Label",
            info="Label for document nodes in the graph",
            value="Document",
            required=False,
        ),
        MessageTextInput(
            name="text_property",
            display_name="Text Property",
            info="Property name for document text",
            value="text",
            required=False,
        ),
        MessageTextInput(
            name="embedding_property",
            display_name="Embedding Property",
            info="Property name for embeddings",
            value="embedding",
            required=False,
        ),
        DataInput(
            name="documents",
            display_name="Documents",
            info="Optional documents to add to the store",
            is_list=True,
            required=False,
        ),
        IntInput(
            name="k",
            display_name="Number of Results",
            info="Number of results for similarity search",
            value=4,
            required=False,
        ),
    ]

    outputs = [
        Output(
            display_name="Vector Store",
            name="vectorstore",
            method="build_vectorstore",
        ),
        Output(
            display_name="Retriever",
            name="retriever",
            method="build_retriever",
        ),
    ]

    def build_vectorstore(self) -> NexusVectorStore:
        """Build and return a NexusVectorStore instance."""
        vectorstore = NexusVectorStore(
            client=self.client,
            embedding=self.embedding,
            label=self.label or "Document",
            text_property=self.text_property or "text",
            embedding_property=self.embedding_property or "embedding",
        )

        # Add documents if provided
        if self.documents:
            docs = self._convert_to_documents(self.documents)
            if docs:
                vectorstore.add_documents(docs)

        return vectorstore

    def build_retriever(self):
        """Build and return a retriever from the vector store."""
        vectorstore = self.build_vectorstore()
        return vectorstore.as_retriever(
            search_kwargs={"k": self.k or 4}
        )

    def _convert_to_documents(self, data_list: List[Any]) -> List[Document]:
        """Convert Data objects to LangChain Documents."""
        documents = []

        for item in data_list:
            if isinstance(item, Document):
                documents.append(item)
            elif isinstance(item, Data):
                # Convert Data to Document
                text = item.data.get("text", item.data.get("content", str(item.data)))
                metadata = {k: v for k, v in item.data.items() if k not in ["text", "content"]}
                documents.append(Document(page_content=text, metadata=metadata))
            elif isinstance(item, dict):
                text = item.get("text", item.get("content", str(item)))
                metadata = {k: v for k, v in item.items() if k not in ["text", "content"]}
                documents.append(Document(page_content=text, metadata=metadata))
            elif isinstance(item, str):
                documents.append(Document(page_content=item))

        return documents


class NexusVectorSearchComponent(Component):
    """LangFlow component for Nexus Vector Search.

    This component performs similarity search on documents
    stored in Nexus vector store.

    Inputs:
        vectorstore: NexusVectorStore instance
        query: Search query text
        k: Number of results to return

    Outputs:
        documents: List of matching documents
        results: Search results with scores
    """

    display_name = "Nexus Vector Search"
    description = "Search for similar documents in Nexus vector store"
    icon = "search"
    name = "NexusVectorSearch"

    inputs = [
        HandleInput(
            name="vectorstore",
            display_name="Vector Store",
            info="NexusVectorStore instance",
            input_types=["NexusVectorStore"],
            required=True,
        ),
        MessageTextInput(
            name="query",
            display_name="Search Query",
            info="Query text for similarity search",
            required=True,
        ),
        IntInput(
            name="k",
            display_name="Number of Results",
            info="Number of results to return",
            value=4,
            required=False,
        ),
    ]

    outputs = [
        Output(
            display_name="Documents",
            name="documents",
            method="search_documents",
        ),
        Output(
            display_name="Results with Scores",
            name="results",
            method="search_with_scores",
        ),
    ]

    def search_documents(self) -> List[Document]:
        """Search for similar documents."""
        return self.vectorstore.similarity_search(
            query=self.query,
            k=self.k or 4,
        )

    def search_with_scores(self) -> List[Data]:
        """Search for similar documents with scores."""
        results = self.vectorstore.similarity_search_with_score(
            query=self.query,
            k=self.k or 4,
        )

        return [
            Data(
                data={
                    "content": doc.page_content,
                    "metadata": doc.metadata,
                    "score": score,
                }
            )
            for doc, score in results
        ]
