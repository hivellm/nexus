/**
 * Live integration tests for external-id node operations (Phase10 §3).
 *
 * These tests run against a real Nexus server. Gate on NEXUS_LIVE_HOST:
 *
 *   NEXUS_LIVE_HOST=http://localhost:15474 npx vitest run tests/external-id.live.test.ts
 *
 * When NEXUS_LIVE_HOST is unset, every test is skipped so unit-only CI
 * passes without a running container.
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { NexusClient } from '../src/client';
import type { CreateNodeResponse, GetNodeByExternalIdResponse } from '../src/types';

// ---------------------------------------------------------------------------
// Gate: skip the entire suite if no live host is configured
// ---------------------------------------------------------------------------

const LIVE_HOST = process.env.NEXUS_LIVE_HOST;
const itLive = LIVE_HOST ? it : it.skip;

// Unique hex strings that are valid for each variant length constraint
const HEX64 = '1'.repeat(64);   // 64 hex chars = sha256 / blake3
const HEX128 = '2'.repeat(128); // 128 hex chars = sha512
const HEX8 = 'deadbeef';        // 4 bytes — valid bytes payload

/** Unique UUID external id to avoid catalog collisions on repeated runs. */
function uniqueExtId(prefix: string): string {
  const uid =
    typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
      ? crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  if (prefix === 'uuid') return `uuid:${uid}`;
  if (prefix === 'str') return `str:live-ts-${uid}`;
  if (prefix === 'sha256') return `sha256:${HEX64}`.slice(0, 7 + 64); // fixed length
  throw new Error(`uniqueExtId: unsupported prefix ${prefix}`);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('External-id live integration tests (Phase10 §3)', () => {
  let client: NexusClient;

  beforeAll(() => {
    if (!LIVE_HOST) return;
    client = new NexusClient({ baseUrl: LIVE_HOST });
  });

  // ── 3.2  All six ExternalId variants ─────────────────────────────────────

  itLive('should round-trip a sha256 external id', async () => {
    const extId = `sha256:${HEX64}`;
    const create: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsSha256'],
      { source: 'phase10-ts-live', variant: 'sha256' },
      extId,
      'match',
    );
    expect(create.error).toBeUndefined();
    expect(typeof create.node_id).toBe('number');
    expect(create.node_id).toBeGreaterThanOrEqual(0);

    const lookup: GetNodeByExternalIdResponse = await client.getNodeByExternalId(extId);
    expect(lookup.error).toBeUndefined();
    expect(lookup.node).not.toBeNull();
    expect(lookup.node?.id).toBe(create.node_id);
  });

  itLive('should round-trip a blake3 external id', async () => {
    const extId = `blake3:${HEX64}`;
    const create: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsBlake3'],
      { source: 'phase10-ts-live', variant: 'blake3' },
      extId,
      'match',
    );
    expect(create.error).toBeUndefined();
    expect(typeof create.node_id).toBe('number');

    const lookup: GetNodeByExternalIdResponse = await client.getNodeByExternalId(extId);
    expect(lookup.node).not.toBeNull();
    expect(lookup.node?.id).toBe(create.node_id);
  });

  itLive('should round-trip a sha512 external id', async () => {
    const extId = `sha512:${HEX128}`;
    const create: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsSha512'],
      { source: 'phase10-ts-live', variant: 'sha512' },
      extId,
      'match',
    );
    expect(create.error).toBeUndefined();
    expect(typeof create.node_id).toBe('number');

    const lookup: GetNodeByExternalIdResponse = await client.getNodeByExternalId(extId);
    expect(lookup.node).not.toBeNull();
    expect(lookup.node?.id).toBe(create.node_id);
  });

  itLive('should round-trip a uuid external id', async () => {
    const extId = uniqueExtId('uuid');
    const create: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsUuid'],
      { source: 'phase10-ts-live', variant: 'uuid' },
      extId,
    );
    expect(create.error).toBeUndefined();
    expect(typeof create.node_id).toBe('number');

    const lookup: GetNodeByExternalIdResponse = await client.getNodeByExternalId(extId);
    expect(lookup.node).not.toBeNull();
    expect(lookup.node?.id).toBe(create.node_id);
  });

  itLive('should round-trip a str external id', async () => {
    const extId = uniqueExtId('str');
    const create: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsStr'],
      { source: 'phase10-ts-live', variant: 'str' },
      extId,
    );
    expect(create.error).toBeUndefined();
    expect(typeof create.node_id).toBe('number');

    const lookup: GetNodeByExternalIdResponse = await client.getNodeByExternalId(extId);
    expect(lookup.node).not.toBeNull();
    expect(lookup.node?.id).toBe(create.node_id);
  });

  itLive('should round-trip a bytes external id', async () => {
    const extId = `bytes:${HEX8}`;
    const create: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsBytes'],
      { source: 'phase10-ts-live', variant: 'bytes' },
      extId,
      'match',
    );
    expect(create.error).toBeUndefined();
    expect(typeof create.node_id).toBe('number');

    const lookup: GetNodeByExternalIdResponse = await client.getNodeByExternalId(extId);
    expect(lookup.node).not.toBeNull();
    expect(lookup.node?.id).toBe(create.node_id);
  });

  // ── 3.3  Conflict policies ────────────────────────────────────────────────

  itLive('should reject a duplicate external id with policy "error" (default)', async () => {
    const extId = uniqueExtId('uuid');
    // First create succeeds
    const first: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsConflict'],
      { attempt: 1 },
      extId,
    );
    expect(first.error).toBeUndefined();
    expect(typeof first.node_id).toBe('number');

    // Second create with the same id and no policy must fail
    const second: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsConflict'],
      { attempt: 2 },
      extId,
    );
    expect(second.error).toBeTruthy();
  });

  itLive('should return the existing node id with policy "match"', async () => {
    const extId = uniqueExtId('uuid');
    const first: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsMatch'],
      { step: 'first' },
      extId,
    );
    expect(first.error).toBeUndefined();
    const originalId = first.node_id;

    const second: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsMatch'],
      { step: 'second' },
      extId,
      'match',
    );
    expect(second.error).toBeUndefined();
    expect(second.node_id).toBe(originalId);
  });

  itLive('should overwrite properties with policy "replace" (regression guard fd001344)', async () => {
    const extId = uniqueExtId('uuid');
    // Create with initial property value
    const first: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsReplace'],
      { value: 'original' },
      extId,
    );
    expect(first.error).toBeUndefined();
    const nodeId = first.node_id;

    // Replace with updated property value
    const second: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsReplace'],
      { value: 'updated' },
      extId,
      'replace',
    );
    expect(second.error).toBeUndefined();
    // Same node id must be returned
    expect(second.node_id).toBe(nodeId);

    // Verify the property actually changed by reading back via getNodeByExternalId
    const lookup: GetNodeByExternalIdResponse = await client.getNodeByExternalId(extId);
    expect(lookup.node).not.toBeNull();
    expect(lookup.node?.id).toBe(nodeId);
    // The replace must have written the new property (fd001344 regression guard)
    const props = lookup.node?.properties as Record<string, unknown>;
    expect(props?.value).toBe('updated');
  });

  // ── 3.4  Cypher _id round-trip ────────────────────────────────────────────

  itLive('should project _id as prefixed string via executeCypher CREATE RETURN n._id', async () => {
    const extId = uniqueExtId('uuid');
    const result = await client.executeCypher(
      `CREATE (n:LiveTsCypher {_id: '${extId}', tag: 'phase10-cypher-test'}) RETURN n._id`,
    );
    // The server projects _id via the first (and only) returned column.
    // The alias may be "n._id" or "result" depending on the server's
    // expression-alias normalisation — access by position to be robust.
    expect(result.columns.length).toBeGreaterThanOrEqual(1);
    expect(result.rows.length).toBeGreaterThanOrEqual(1);
    const firstCol = result.columns[0];
    const projected = result.rows[0][firstCol];
    expect(projected).toBe(extId);
  });

  itLive('should project _id as null for a node without an external id', async () => {
    // Create a plain node, then read _id back — must be null not an error
    const label = `LiveTsNoExtId_${Date.now()}`;
    await client.executeCypher(
      `CREATE (n:${label} {name: 'plain-node'})`,
    );
    const result = await client.executeCypher(
      `MATCH (n:${label}) RETURN n._id LIMIT 1`,
    );
    expect(result.rows.length).toBeGreaterThanOrEqual(1);
    // Access by first column name since the server may alias _id projection
    const firstCol = result.columns[0];
    const val = result.rows[0][firstCol];
    expect(val === null || val === undefined).toBe(true);
  });

  itLive('should resolve a node created via Cypher by its external id via getNodeByExternalId', async () => {
    const extId = uniqueExtId('uuid');
    // Create via Cypher
    await client.executeCypher(
      `CREATE (n:LiveTsCypherLookup {_id: '${extId}', name: 'cypher-created'})`,
    );
    // Look up via SDK helper
    const lookup: GetNodeByExternalIdResponse = await client.getNodeByExternalId(extId);
    expect(lookup.node).not.toBeNull();
    expect(typeof lookup.node?.id).toBe('number');
  });

  // ── 3.5  Length-cap rejection ─────────────────────────────────────────────

  itLive('should reject a str external id longer than 256 bytes', async () => {
    const tooLong = `str:${'a'.repeat(257)}`;
    const result: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsCapStr'],
      {},
      tooLong,
    );
    expect(result.error).toBeTruthy();
  });

  itLive('should reject a bytes external id longer than 64 bytes (65 bytes = 130 hex chars)', async () => {
    const tooLong = `bytes:${'ff'.repeat(65)}`; // 65 bytes
    const result: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsCapBytes'],
      {},
      tooLong,
    );
    expect(result.error).toBeTruthy();
  });

  itLive('should reject an empty uuid payload', async () => {
    const empty = 'uuid:';
    const result: CreateNodeResponse = await client.createNodeWithExternalId(
      ['LiveTsEmptyUuid'],
      {},
      empty,
    );
    expect(result.error).toBeTruthy();
  });

  // ── Absent external id returns null node (not an HTTP error) ─────────────

  itLive('should return null node for an external id that was never registered', async () => {
    const absent = `uuid:${
      typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
        ? crypto.randomUUID()
        : `00000000-0000-0000-0000-${Date.now()}`
    }`;
    const result: GetNodeByExternalIdResponse = await client.getNodeByExternalId(absent);
    expect(result.error).toBeUndefined();
    expect(result.node).toBeNull();
  });
});
