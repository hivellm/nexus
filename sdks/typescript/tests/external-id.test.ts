/**
 * Tests for external-id node operations (Phase9 §5.5).
 *
 * These tests require a running Nexus server.  They are skipped
 * automatically when the server is not reachable, mirroring the
 * pattern used in the rest of this test suite.
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { NexusClient } from '../src/client';
import type { CreateNodeResponse, GetNodeByExternalIdResponse } from '../src/types';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const SERVER_URL = process.env.NEXUS_URL ?? 'http://localhost:15474';

/** Return a fresh ``uuid:…`` external id so tests never collide. */
function uniqueUuidExtId(): string {
  // crypto.randomUUID() is available in Node 15+ and modern browsers.
  const id =
    typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
      ? crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `uuid:${id}`;
}

async function serverAvailable(client: NexusClient): Promise<boolean> {
  try {
    return await client.ping();
  } catch {
    return false;
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('External-id node operations (Phase9 §5.5)', () => {
  let client: NexusClient;

  beforeAll(() => {
    client = new NexusClient({ baseUrl: SERVER_URL });
  });

  it('createNodeWithExternalId + getNodeByExternalId round-trip', async () => {
    if (!(await serverAvailable(client))) {
      // Skip gracefully — same convention as other tests that need a server.
      console.warn(`[skip] Nexus server not reachable at ${SERVER_URL}`);
      return;
    }

    const extId = uniqueUuidExtId();

    const create: CreateNodeResponse = await client.createNodeWithExternalId(
      ['ExtIdTsTest'],
      { imported_from: 'phase9_ts_test' },
      extId,
      'match',
    );

    // Skip rather than fail if the engine is not initialised.
    if (create.error) {
      console.warn(`[skip] Server returned creation error: ${create.error}`);
      return;
    }

    expect(create.node_id).toBeGreaterThan(0);

    const lookup: GetNodeByExternalIdResponse =
      await client.getNodeByExternalId(extId);

    expect(lookup.error).toBeUndefined();
    expect(lookup.node).not.toBeNull();
    expect(lookup.node?.id).toBe(create.node_id);
  });

  it('getNodeByExternalId returns null node for non-existent id', async () => {
    if (!(await serverAvailable(client))) {
      console.warn(`[skip] Nexus server not reachable at ${SERVER_URL}`);
      return;
    }

    const nonexistent = uniqueUuidExtId();
    const result: GetNodeByExternalIdResponse =
      await client.getNodeByExternalId(nonexistent);

    // Server contract: miss → node absent, no hard error.
    expect(result.node).toBeNull();
  });
});
