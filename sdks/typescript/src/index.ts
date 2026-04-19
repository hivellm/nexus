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
} from './types';

export { defaultLocalEndpoint, parseEndpoint, endpointToString } from './transports/endpoint';
export { mapCommand } from './transports/command-map';
export type { Endpoint } from './transports/endpoint';
export type { NexusValue, Transport, TransportRequest, TransportResponse } from './transports/types';

