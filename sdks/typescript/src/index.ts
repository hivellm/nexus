/**
 * @hivellm/nexus-sdk
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
} from './types';

