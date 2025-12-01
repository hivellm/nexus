"""Data models for Nexus SDK."""

from typing import Any, Dict, List, Optional, Union
from pydantic import BaseModel, Field


class QueryResult(BaseModel):
    """Cypher query result."""

    columns: List[str] = Field(default_factory=list)
    rows: List[List[Any]] = Field(default_factory=list)
    execution_time_ms: Optional[int] = Field(None, alias="execution_time_ms")
    error: Optional[str] = None

    class Config:
        populate_by_name = True


class DatabaseStats(BaseModel):
    """Database statistics."""

    catalog: Dict[str, Any] = Field(default_factory=dict)
    storage: Dict[str, Any] = Field(default_factory=dict)


class Node(BaseModel):
    """Graph node."""

    id: int
    labels: List[str] = Field(default_factory=list)
    properties: Dict[str, Any] = Field(default_factory=dict)


class Relationship(BaseModel):
    """Graph relationship."""

    id: int
    type: str
    source_id: int
    target_id: int
    properties: Dict[str, Any] = Field(default_factory=dict)


class CreateNodeRequest(BaseModel):
    """Request to create a node."""

    labels: List[str] = Field(default_factory=list)
    properties: Dict[str, Any] = Field(default_factory=dict)


class CreateNodeResponse(BaseModel):
    """Response from creating a node."""

    node_id: int
    node: Optional[Node] = None
    error: Optional[str] = None


class UpdateNodeRequest(BaseModel):
    """Request to update a node."""

    node_id: int
    labels: Optional[List[str]] = None
    properties: Optional[Dict[str, Any]] = None


class UpdateNodeResponse(BaseModel):
    """Response from updating a node."""

    node: Optional[Node] = None
    error: Optional[str] = None


class DeleteNodeRequest(BaseModel):
    """Request to delete a node."""

    node_id: int


class DeleteNodeResponse(BaseModel):
    """Response from deleting a node."""

    success: bool = True
    error: Optional[str] = None


class CreateRelationshipRequest(BaseModel):
    """Request to create a relationship."""

    source_id: int
    target_id: int
    rel_type: str
    properties: Dict[str, Any] = Field(default_factory=dict)


class CreateRelationshipResponse(BaseModel):
    """Response from creating a relationship."""

    relationship_id: int
    relationship: Optional[Relationship] = None
    error: Optional[str] = None


class UpdateRelationshipRequest(BaseModel):
    """Request to update a relationship."""

    relationship_id: int
    properties: Dict[str, Any]


class UpdateRelationshipResponse(BaseModel):
    """Response from updating a relationship."""

    relationship: Optional[Relationship] = None
    error: Optional[str] = None


class DeleteRelationshipRequest(BaseModel):
    """Request to delete a relationship."""

    relationship_id: int


class DeleteRelationshipResponse(BaseModel):
    """Response from deleting a relationship."""

    success: bool = True
    error: Optional[str] = None


class LabelResponse(BaseModel):
    """Response for label operations."""

    labels: List[str] = Field(default_factory=list)
    error: Optional[str] = None


class RelTypeResponse(BaseModel):
    """Response for relationship type operations."""

    types: List[str] = Field(default_factory=list)
    error: Optional[str] = None


class TransactionResponse(BaseModel):
    """Response for transaction operations."""

    transaction_id: Optional[str] = None
    success: bool = True
    error: Optional[str] = None


# Type alias for Value (can be any JSON-serializable value)
Value = Union[None, bool, int, float, str, List[Any], Dict[str, Any]]


class BatchNode(BaseModel):
    """Batch node definition."""

    labels: List[str] = Field(default_factory=list)
    properties: Dict[str, Any] = Field(default_factory=dict)


class BatchRelationship(BaseModel):
    """Batch relationship definition."""

    source_id: int
    target_id: int
    rel_type: str
    properties: Dict[str, Any] = Field(default_factory=dict)


class BatchCreateNodesRequest(BaseModel):
    """Request to batch create nodes."""

    nodes: List[BatchNode] = Field(default_factory=list)


class BatchCreateNodesResponse(BaseModel):
    """Response from batch creating nodes."""

    node_ids: List[int] = Field(default_factory=list)
    message: str = ""
    error: Optional[str] = None


class BatchCreateRelationshipsRequest(BaseModel):
    """Request to batch create relationships."""

    relationships: List[BatchRelationship] = Field(default_factory=list)


class BatchCreateRelationshipsResponse(BaseModel):
    """Response from batch creating relationships."""

    rel_ids: List[int] = Field(default_factory=list)
    message: str = ""
    error: Optional[str] = None


class QueryStatisticsSummary(BaseModel):
    """Query statistics summary."""

    total_queries: int = 0
    successful_queries: int = 0
    failed_queries: int = 0
    total_execution_time_ms: int = 0
    average_execution_time_ms: int = 0
    min_execution_time_ms: int = 0
    max_execution_time_ms: int = 0
    slow_query_count: int = 0


class QueryPatternStats(BaseModel):
    """Query pattern statistics."""

    pattern: str = ""
    count: int = 0
    avg_time_ms: int = 0
    min_time_ms: int = 0
    max_time_ms: int = 0
    success_count: int = 0
    failure_count: int = 0


class QueryStatisticsResponse(BaseModel):
    """Query statistics response."""

    statistics: QueryStatisticsSummary = Field(default_factory=QueryStatisticsSummary)
    patterns: List[QueryPatternStats] = Field(default_factory=list)


class SlowQueryRecord(BaseModel):
    """Slow query record."""

    query: str = ""
    execution_time_ms: int = 0
    timestamp: str = ""
    success: bool = True
    error: Optional[str] = None
    rows_returned: int = 0


class SlowQueriesResponse(BaseModel):
    """Slow queries response."""

    count: int = 0
    queries: List[SlowQueryRecord] = Field(default_factory=list)


class PlanCacheStatisticsResponse(BaseModel):
    """Plan cache statistics response."""

    cached_plans: int = 0
    max_size: int = 0
    current_memory_bytes: int = 0
    max_memory_bytes: int = 0
    hit_rate: float = 0.0


# Database management models

class DatabaseInfo(BaseModel):
    """Database information."""

    name: str
    path: str = ""
    created_at: int = 0
    node_count: int = 0
    relationship_count: int = 0
    storage_size: int = 0


class ListDatabasesResponse(BaseModel):
    """Response for listing databases."""

    databases: List[DatabaseInfo] = Field(default_factory=list)
    default_database: str = "neo4j"


class CreateDatabaseRequest(BaseModel):
    """Request to create a database."""

    name: str


class CreateDatabaseResponse(BaseModel):
    """Response from creating a database."""

    success: bool = True
    name: str = ""
    message: str = ""


class DropDatabaseResponse(BaseModel):
    """Response from dropping a database."""

    success: bool = True
    message: str = ""


class SessionDatabaseResponse(BaseModel):
    """Response for session database operations."""

    database: str = ""


class SwitchDatabaseRequest(BaseModel):
    """Request to switch database."""

    name: str


class SwitchDatabaseResponse(BaseModel):
    """Response from switching database."""

    success: bool = True
    message: str = ""

