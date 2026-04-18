"""Nexus Vector Store implementation for LangChain."""

from __future__ import annotations

import asyncio
import uuid
from typing import Any, Callable, Dict, Iterable, List, Optional, Tuple, Type

from langchain_core.documents import Document
from langchain_core.embeddings import Embeddings
from langchain_core.vectorstores import VectorStore

from langchain_nexus.client import NexusClient


class NexusVectorStore(VectorStore):
    """LangChain VectorStore implementation for Nexus graph database.

    This vector store stores documents as nodes in Nexus with their embeddings,
    enabling both vector similarity search and graph-based retrieval.

    Args:
        client: NexusClient instance for database connection
        embedding: Embeddings model for generating vectors
        label: Node label for document nodes (default: "Document")
        text_property: Property name for document text (default: "text")
        embedding_property: Property name for embedding vector (default: "embedding")
        metadata_prefix: Prefix for metadata properties (default: "metadata_")

    Example:
        >>> from langchain_openai import OpenAIEmbeddings
        >>> from langchain_nexus import NexusVectorStore, NexusClient
        >>>
        >>> client = NexusClient("http://localhost:15474")
        >>> embeddings = OpenAIEmbeddings()
        >>> vectorstore = NexusVectorStore(client, embeddings)
        >>>
        >>> # Add documents
        >>> vectorstore.add_texts(["Hello world", "Goodbye world"])
        >>>
        >>> # Search
        >>> docs = vectorstore.similarity_search("Hello", k=1)
    """

    def __init__(
        self,
        client: NexusClient,
        embedding: Embeddings,
        label: str = "Document",
        text_property: str = "text",
        embedding_property: str = "embedding",
        metadata_prefix: str = "metadata_",
    ):
        self._client = client
        self._embedding = embedding
        self._label = label
        self._text_property = text_property
        self._embedding_property = embedding_property
        self._metadata_prefix = metadata_prefix

    @property
    def embeddings(self) -> Optional[Embeddings]:
        """Return the embeddings model."""
        return self._embedding

    def _create_node_properties(
        self,
        text: str,
        embedding: List[float],
        metadata: Optional[Dict[str, Any]] = None,
        doc_id: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Create node properties from text, embedding, and metadata."""
        properties = {
            self._text_property: text,
            self._embedding_property: embedding,
            "_id": doc_id or str(uuid.uuid4()),
        }

        if metadata:
            for key, value in metadata.items():
                # Only include serializable values
                if isinstance(value, (str, int, float, bool, list)):
                    properties[f"{self._metadata_prefix}{key}"] = value

        return properties

    def _extract_metadata(self, properties: Dict[str, Any]) -> Dict[str, Any]:
        """Extract metadata from node properties."""
        metadata = {}
        prefix_len = len(self._metadata_prefix)

        for key, value in properties.items():
            if key.startswith(self._metadata_prefix):
                metadata[key[prefix_len:]] = value

        return metadata

    def add_texts(
        self,
        texts: Iterable[str],
        metadatas: Optional[List[Dict[str, Any]]] = None,
        ids: Optional[List[str]] = None,
        **kwargs: Any,
    ) -> List[str]:
        """Add texts to the vector store.

        Args:
            texts: Iterable of texts to add
            metadatas: Optional list of metadata dicts for each text
            ids: Optional list of IDs for each text
            **kwargs: Additional arguments (unused)

        Returns:
            List of IDs for the added texts
        """
        texts_list = list(texts)
        if not texts_list:
            return []

        # Generate embeddings
        embeddings = self._embedding.embed_documents(texts_list)

        # Prepare metadata and IDs
        if metadatas is None:
            metadatas = [{} for _ in texts_list]
        if ids is None:
            ids = [str(uuid.uuid4()) for _ in texts_list]

        # Create nodes
        result_ids = []
        for text, embedding, metadata, doc_id in zip(
            texts_list, embeddings, metadatas, ids
        ):
            properties = self._create_node_properties(text, embedding, metadata, doc_id)
            node_id = self._client.create_node_sync([self._label], properties)
            result_ids.append(doc_id)

        return result_ids

    async def aadd_texts(
        self,
        texts: Iterable[str],
        metadatas: Optional[List[Dict[str, Any]]] = None,
        ids: Optional[List[str]] = None,
        **kwargs: Any,
    ) -> List[str]:
        """Add texts to the vector store asynchronously.

        Args:
            texts: Iterable of texts to add
            metadatas: Optional list of metadata dicts for each text
            ids: Optional list of IDs for each text
            **kwargs: Additional arguments (unused)

        Returns:
            List of IDs for the added texts
        """
        texts_list = list(texts)
        if not texts_list:
            return []

        # Generate embeddings
        embeddings = await self._embedding.aembed_documents(texts_list)

        # Prepare metadata and IDs
        if metadatas is None:
            metadatas = [{} for _ in texts_list]
        if ids is None:
            ids = [str(uuid.uuid4()) for _ in texts_list]

        # Create nodes
        result_ids = []
        for text, embedding, metadata, doc_id in zip(
            texts_list, embeddings, metadatas, ids
        ):
            properties = self._create_node_properties(text, embedding, metadata, doc_id)
            node_id = await self._client.create_node([self._label], properties)
            result_ids.append(doc_id)

        return result_ids

    def add_documents(
        self,
        documents: List[Document],
        ids: Optional[List[str]] = None,
        **kwargs: Any,
    ) -> List[str]:
        """Add documents to the vector store.

        Args:
            documents: List of Document objects to add
            ids: Optional list of IDs for each document
            **kwargs: Additional arguments

        Returns:
            List of IDs for the added documents
        """
        texts = [doc.page_content for doc in documents]
        metadatas = [doc.metadata for doc in documents]
        return self.add_texts(texts, metadatas=metadatas, ids=ids, **kwargs)

    async def aadd_documents(
        self,
        documents: List[Document],
        ids: Optional[List[str]] = None,
        **kwargs: Any,
    ) -> List[str]:
        """Add documents to the vector store asynchronously.

        Args:
            documents: List of Document objects to add
            ids: Optional list of IDs for each document
            **kwargs: Additional arguments

        Returns:
            List of IDs for the added documents
        """
        texts = [doc.page_content for doc in documents]
        metadatas = [doc.metadata for doc in documents]
        return await self.aadd_texts(texts, metadatas=metadatas, ids=ids, **kwargs)

    def similarity_search(
        self,
        query: str,
        k: int = 4,
        filter: Optional[Dict[str, Any]] = None,
        **kwargs: Any,
    ) -> List[Document]:
        """Search for similar documents.

        Args:
            query: Query text
            k: Number of results to return
            filter: Optional metadata filter (not yet implemented)
            **kwargs: Additional arguments

        Returns:
            List of similar documents
        """
        docs_with_scores = self.similarity_search_with_score(query, k, filter, **kwargs)
        return [doc for doc, _ in docs_with_scores]

    async def asimilarity_search(
        self,
        query: str,
        k: int = 4,
        filter: Optional[Dict[str, Any]] = None,
        **kwargs: Any,
    ) -> List[Document]:
        """Search for similar documents asynchronously.

        Args:
            query: Query text
            k: Number of results to return
            filter: Optional metadata filter
            **kwargs: Additional arguments

        Returns:
            List of similar documents
        """
        docs_with_scores = await self.asimilarity_search_with_score(
            query, k, filter, **kwargs
        )
        return [doc for doc, _ in docs_with_scores]

    def similarity_search_with_score(
        self,
        query: str,
        k: int = 4,
        filter: Optional[Dict[str, Any]] = None,
        **kwargs: Any,
    ) -> List[Tuple[Document, float]]:
        """Search for similar documents with relevance scores.

        Args:
            query: Query text
            k: Number of results to return
            filter: Optional metadata filter
            **kwargs: Additional arguments

        Returns:
            List of (document, score) tuples
        """
        # Generate query embedding
        query_embedding = self._embedding.embed_query(query)

        # Perform KNN search
        results = self._client.knn_search_sync(
            label=self._label,
            vector=query_embedding,
            k=k,
            property_name=self._embedding_property,
        )

        # Convert results to documents
        docs_with_scores = []
        for result in results:
            properties = result.get("properties", result)
            text = properties.get(self._text_property, "")
            metadata = self._extract_metadata(properties)
            metadata["_node_id"] = result.get("node_id", result.get("id"))
            score = result.get("score", result.get("similarity", 0.0))

            doc = Document(page_content=text, metadata=metadata)
            docs_with_scores.append((doc, score))

        return docs_with_scores

    async def asimilarity_search_with_score(
        self,
        query: str,
        k: int = 4,
        filter: Optional[Dict[str, Any]] = None,
        **kwargs: Any,
    ) -> List[Tuple[Document, float]]:
        """Search for similar documents with relevance scores asynchronously.

        Args:
            query: Query text
            k: Number of results to return
            filter: Optional metadata filter
            **kwargs: Additional arguments

        Returns:
            List of (document, score) tuples
        """
        # Generate query embedding
        query_embedding = await self._embedding.aembed_query(query)

        # Perform KNN search
        results = await self._client.knn_search(
            label=self._label,
            vector=query_embedding,
            k=k,
            property_name=self._embedding_property,
        )

        # Convert results to documents
        docs_with_scores = []
        for result in results:
            properties = result.get("properties", result)
            text = properties.get(self._text_property, "")
            metadata = self._extract_metadata(properties)
            metadata["_node_id"] = result.get("node_id", result.get("id"))
            score = result.get("score", result.get("similarity", 0.0))

            doc = Document(page_content=text, metadata=metadata)
            docs_with_scores.append((doc, score))

        return docs_with_scores

    def similarity_search_by_vector(
        self,
        embedding: List[float],
        k: int = 4,
        filter: Optional[Dict[str, Any]] = None,
        **kwargs: Any,
    ) -> List[Document]:
        """Search for similar documents by vector.

        Args:
            embedding: Query vector
            k: Number of results to return
            filter: Optional metadata filter
            **kwargs: Additional arguments

        Returns:
            List of similar documents
        """
        results = self._client.knn_search_sync(
            label=self._label,
            vector=embedding,
            k=k,
            property_name=self._embedding_property,
        )

        docs = []
        for result in results:
            properties = result.get("properties", result)
            text = properties.get(self._text_property, "")
            metadata = self._extract_metadata(properties)
            metadata["_node_id"] = result.get("node_id", result.get("id"))

            doc = Document(page_content=text, metadata=metadata)
            docs.append(doc)

        return docs

    async def asimilarity_search_by_vector(
        self,
        embedding: List[float],
        k: int = 4,
        filter: Optional[Dict[str, Any]] = None,
        **kwargs: Any,
    ) -> List[Document]:
        """Search for similar documents by vector asynchronously.

        Args:
            embedding: Query vector
            k: Number of results to return
            filter: Optional metadata filter
            **kwargs: Additional arguments

        Returns:
            List of similar documents
        """
        results = await self._client.knn_search(
            label=self._label,
            vector=embedding,
            k=k,
            property_name=self._embedding_property,
        )

        docs = []
        for result in results:
            properties = result.get("properties", result)
            text = properties.get(self._text_property, "")
            metadata = self._extract_metadata(properties)
            metadata["_node_id"] = result.get("node_id", result.get("id"))

            doc = Document(page_content=text, metadata=metadata)
            docs.append(doc)

        return docs

    def delete(
        self,
        ids: Optional[List[str]] = None,
        **kwargs: Any,
    ) -> Optional[bool]:
        """Delete documents by ID.

        Args:
            ids: List of document IDs to delete
            **kwargs: Additional arguments

        Returns:
            True if successful, None if not implemented
        """
        if not ids:
            return None

        for doc_id in ids:
            query = f"""
            MATCH (n:{self._label} {{_id: $id}})
            DETACH DELETE n
            """
            self._client.execute_cypher_sync(query, {"id": doc_id})

        return True

    async def adelete(
        self,
        ids: Optional[List[str]] = None,
        **kwargs: Any,
    ) -> Optional[bool]:
        """Delete documents by ID asynchronously.

        Args:
            ids: List of document IDs to delete
            **kwargs: Additional arguments

        Returns:
            True if successful, None if not implemented
        """
        if not ids:
            return None

        for doc_id in ids:
            query = f"""
            MATCH (n:{self._label} {{_id: $id}})
            DETACH DELETE n
            """
            await self._client.execute_cypher(query, {"id": doc_id})

        return True

    @classmethod
    def from_texts(
        cls: Type["NexusVectorStore"],
        texts: List[str],
        embedding: Embeddings,
        metadatas: Optional[List[Dict[str, Any]]] = None,
        *,
        url: str = "http://localhost:15474",
        api_key: Optional[str] = None,
        label: str = "Document",
        **kwargs: Any,
    ) -> "NexusVectorStore":
        """Create a NexusVectorStore from texts.

        Args:
            texts: List of texts to add
            embedding: Embeddings model
            metadatas: Optional list of metadata dicts
            url: Nexus server URL
            api_key: Optional API key
            label: Node label for documents
            **kwargs: Additional arguments

        Returns:
            NexusVectorStore instance with texts added
        """
        client = NexusClient(url=url, api_key=api_key)
        vectorstore = cls(client=client, embedding=embedding, label=label, **kwargs)
        vectorstore.add_texts(texts, metadatas=metadatas)
        return vectorstore

    @classmethod
    async def afrom_texts(
        cls: Type["NexusVectorStore"],
        texts: List[str],
        embedding: Embeddings,
        metadatas: Optional[List[Dict[str, Any]]] = None,
        *,
        url: str = "http://localhost:15474",
        api_key: Optional[str] = None,
        label: str = "Document",
        **kwargs: Any,
    ) -> "NexusVectorStore":
        """Create a NexusVectorStore from texts asynchronously.

        Args:
            texts: List of texts to add
            embedding: Embeddings model
            metadatas: Optional list of metadata dicts
            url: Nexus server URL
            api_key: Optional API key
            label: Node label for documents
            **kwargs: Additional arguments

        Returns:
            NexusVectorStore instance with texts added
        """
        client = NexusClient(url=url, api_key=api_key)
        vectorstore = cls(client=client, embedding=embedding, label=label, **kwargs)
        await vectorstore.aadd_texts(texts, metadatas=metadatas)
        return vectorstore

    @classmethod
    def from_documents(
        cls: Type["NexusVectorStore"],
        documents: List[Document],
        embedding: Embeddings,
        *,
        url: str = "http://localhost:15474",
        api_key: Optional[str] = None,
        label: str = "Document",
        **kwargs: Any,
    ) -> "NexusVectorStore":
        """Create a NexusVectorStore from documents.

        Args:
            documents: List of documents to add
            embedding: Embeddings model
            url: Nexus server URL
            api_key: Optional API key
            label: Node label for documents
            **kwargs: Additional arguments

        Returns:
            NexusVectorStore instance with documents added
        """
        texts = [doc.page_content for doc in documents]
        metadatas = [doc.metadata for doc in documents]
        return cls.from_texts(
            texts,
            embedding,
            metadatas=metadatas,
            url=url,
            api_key=api_key,
            label=label,
            **kwargs,
        )
