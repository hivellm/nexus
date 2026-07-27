/**
 * @hivehub/nexus-sdk
 * 
 * Official TypeScript/JavaScript SDK for Nexus Graph Database
 * 
 * @packageDocumentation
 */

export { NexusClient } from './client';
export {
    NexusSDKError,
    AuthenticationError,
    ConnectionError,
    QueryExecutionError,
    ValidationError,
} from './errors';
export type {
    AuthConfig,
    NexusConfig,
    TransportMode,
    QueryParams,
    ResultRow,
    QueryResult,
    NodeProperties,
    RelationshipProperties,
    Node,
    Relationship,
    QueryStatistics,
    SchemaInfo,
    IndexInfo,
    TransactionOptions,
    BatchOperation,
    NexusError,
    // Database management types
    DatabaseInfo,
    ListDatabasesResponse,
    CreateDatabaseResponse,
    DropDatabaseResponse,
    SessionDatabaseResponse,
    SwitchDatabaseResponse,
    // External-id node types (Phase9 §5.5)
    CreateNodeWithExternalIdRequest,
    CreateNodeResponse,
    GetNodeByExternalIdResponse,
} from './types';

export { defaultLocalEndpoint, parseEndpoint, endpointToString } from './transports/endpoint';
export { mapCommand } from './transports/command-map';
export type { Endpoint } from './transports/endpoint';
export type { NexusValue, Transport, TransportRequest, TransportResponse } from './transports/types';

// Raw transport surface — for callers driving the wire directly (interop
// harnesses, diagnostics) rather than through `NexusClient`'s sugar layer.
// Mirrors the Rust SDK's `pub mod transport` and the Python SDK's
// `nexus_sdk.transport.rpc` re-export.
export { RpcTransport } from './transports/rpc';
export { nx } from './transports/types';
export type { TransportCredentials } from './transports/types';

