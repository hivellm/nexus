/**
 * `connectionsStore` tests. Covers default-connection seeding,
 * upsert idempotency, removal cascade rules, and the per-row
 * status setter that the health probe loop calls.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { useConnectionsStore, selectCurrentConnection } from './connectionsStore';

beforeEach(() => {
  localStorage.clear();
  // Force the store back to its default shape — two seeded
  // connections (`local` + `cortex`), `local` active.
  useConnectionsStore.setState({
    connections: [
      {
        id: 'local',
        name: 'localhost',
        url: 'http://localhost:15474',
        role: 'standalone',
        status: 'idle',
      },
      {
        id: 'cortex',
        name: 'cortex',
        url: 'http://localhost:15002',
        role: 'standalone',
        status: 'idle',
      },
    ],
    currentConnectionId: 'local',
  });
});

describe('connectionsStore', () => {
  it('selectCurrentConnection resolves the active row', () => {
    const conn = selectCurrentConnection(useConnectionsStore.getState());
    expect(conn?.id).toBe('local');
  });

  it('setCurrent flips the active id', () => {
    useConnectionsStore.getState().setCurrent('cortex');
    expect(useConnectionsStore.getState().currentConnectionId).toBe('cortex');
  });

  it('upsert appends a new connection', () => {
    useConnectionsStore.getState().upsert({
      id: 'staging',
      name: 'staging',
      url: 'http://10.0.0.5:15474',
      role: 'master',
      status: 'idle',
    });
    expect(useConnectionsStore.getState().connections).toHaveLength(3);
  });

  it('upsert with existing id replaces in place', () => {
    useConnectionsStore.getState().upsert({
      id: 'local',
      name: 'localhost-renamed',
      url: 'http://localhost:15474',
      role: 'master',
      status: 'connected',
    });
    const conns = useConnectionsStore.getState().connections;
    expect(conns).toHaveLength(2);
    const local = conns.find((c) => c.id === 'local');
    expect(local?.name).toBe('localhost-renamed');
    expect(local?.role).toBe('master');
  });

  it('remove drops the connection and falls back to the first remaining row when active', () => {
    useConnectionsStore.getState().setCurrent('cortex');
    useConnectionsStore.getState().remove('cortex');
    const s = useConnectionsStore.getState();
    expect(s.connections.map((c) => c.id)).toEqual(['local']);
    expect(s.currentConnectionId).toBe('local');
  });

  it('remove keeps the active id when a non-active row is dropped', () => {
    useConnectionsStore.getState().remove('cortex');
    expect(useConnectionsStore.getState().currentConnectionId).toBe('local');
  });

  it('setStatus updates only the matching row', () => {
    useConnectionsStore.getState().setStatus('cortex', 'connected');
    const cortex = useConnectionsStore
      .getState()
      .connections.find((c) => c.id === 'cortex');
    expect(cortex?.status).toBe('connected');
    const local = useConnectionsStore
      .getState()
      .connections.find((c) => c.id === 'local');
    expect(local?.status).toBe('idle');
  });
});
