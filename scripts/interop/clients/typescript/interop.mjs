#!/usr/bin/env node
/**
 * Interop cell: TypeScript SDK (@hivehub/thunder) against a Thunder-based server.
 *
 * Drives `RpcTransport` directly rather than the `NexusClient` sugar layer --
 * the matrix is about the wire, and the transport is where the wire lives.
 * Mirrors `clients/python/interop.py` step for step.
 *
 *   argv:   <host> <port> <user> <pass>
 *   stdout: one `STEP <name> PASS|FAIL <detail>` line per step
 *   exit:   0 iff every step passed
 *
 * Imported from the SDK's built `dist/` (not `src/`) so `@hivehub/thunder`
 * resolves from the SDK's own `node_modules` without a package.json of its
 * own for this script.
 */

import { RpcTransport, nx } from '../../../../sdks/typescript/dist/index.mjs';

// A vector whose f32-LE encoding is emphatically not valid UTF-8, so a
// transport that quietly round-trips Bytes through a string cannot pass
// the knn_bytes cell.
const VEC = [1.5, -2.5, 3.5, Infinity];
const VEC_BYTES = encodeFloat32LE(VEC);

function encodeFloat32LE(values) {
  const bytes = new Uint8Array(4 * values.length);
  const view = new DataView(bytes.buffer);
  values.forEach((v, i) => view.setFloat32(i * 4, v, true));
  return bytes;
}

function bytesEqual(a, b) {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

function toHex(bytes) {
  return Buffer.from(bytes).toString('hex');
}

function report(step, ok, detail) {
  console.log(`STEP ${step} ${ok ? 'PASS' : 'FAIL'} ${detail}`);
}

/** Look up a string key in a Map-kind NexusValue. */
function mapGet(v, key) {
  if (v.kind !== 'Map') return null;
  for (const [k, val] of v.value) {
    if (k.kind === 'Str' && k.value === key) return val;
  }
  return null;
}

function toNx(v) {
  if (typeof v === 'boolean') return nx.Bool(v);
  if (typeof v === 'bigint') return nx.Int(v);
  if (typeof v === 'number') return Number.isInteger(v) ? nx.Int(v) : nx.Float(v);
  if (v instanceof Uint8Array) return nx.Bytes(v);
  return nx.Str(String(v));
}

function fromNx(v) {
  return v.value;
}

/** Run CYPHER over the transport and return rows as native arrays. */
async function cypherRows(t, query, params) {
  const args = [nx.Str(query)];
  const keys = params ? Object.keys(params) : [];
  if (keys.length > 0) {
    args.push(nx.Map(keys.map((k) => [nx.Str(k), toNx(params[k])])));
  }
  const resp = await t.execute({ command: 'CYPHER', args });
  const err = mapGet(resp.value, 'error');
  if (err !== null && err.kind === 'Str' && err.value) {
    throw new Error(err.value);
  }
  const rows = mapGet(resp.value, 'rows');
  if (rows === null || rows.kind !== 'Array') return [];
  return rows.value
    .filter((row) => row.kind === 'Array')
    .map((row) => row.value.map(fromNx));
}

function describeError(exc) {
  const name = exc && exc.constructor ? exc.constructor.name : 'Error';
  const message = exc && exc.message !== undefined ? exc.message : String(exc);
  return `${name}: ${message}`;
}

async function main() {
  const [, , host, portStr, user, password] = process.argv;
  const endpoint = { scheme: 'nexus', host, port: Number(portStr) };
  let failures = 0;
  let authed;

  // 1. auth -- PING answers before auth; STATS is refused before AUTH and
  //    succeeds after. An unauthenticated transport is one built with no
  //    credentials.
  try {
    const anon = new RpcTransport(endpoint, {});
    const pong = await anon.execute({ command: 'PING', args: [] });
    const pingOk = pong.value.kind === 'Str' && pong.value.value === 'PONG';

    let statsPreRefused = false;
    try {
      await anon.execute({ command: 'STATS', args: [] });
    } catch (exc) {
      const msg = exc && exc.message !== undefined ? String(exc.message) : String(exc);
      statsPreRefused = msg.toLowerCase().includes('auth') || msg.includes('NOAUTH');
    }
    await anon.close();

    authed = new RpcTransport(endpoint, { username: user, password });
    const stats = await authed.execute({ command: 'STATS', args: [] });
    const statsPostOk = stats.value.kind === 'Map' || stats.value.kind === 'Str';

    const ok = pingOk && statsPreRefused && statsPostOk;
    report(
      'auth',
      ok,
      `ping=${pingOk} stats_pre_refused=${statsPreRefused} stats_post=${statsPostOk}`,
    );
    failures += ok ? 0 : 1;
  } catch (exc) {
    report('auth', false, describeError(exc));
    return 1;
  }

  // 2. cypher -- CREATE then MATCH round-trips the id back.
  const marker = 424202;
  try {
    await cypherRows(authed, 'CREATE (n:InteropTs {id: $id}) RETURN n.id', { id: marker });
    const rows = await cypherRows(authed, 'MATCH (n:InteropTs {id: $id}) RETURN n.id', {
      id: marker,
    });
    const got = rows.length > 0 && rows[0].length > 0 ? rows[0][0] : null;
    const ok = got !== null && Number(got) === marker;
    report('cypher', ok, `round-trip id -> ${got === null ? 'null' : String(got)}`);
    failures += ok ? 0 : 1;
  } catch (exc) {
    report('cypher', false, describeError(exc));
    failures += 1;
  }

  // 3. knn_bytes -- a raw f32-LE vector carried as Bytes round-trips
  //    byte-exact (PING echoes its argument), and the client's own float
  //    encoding agrees.
  try {
    const echoed = await authed.execute({ command: 'PING', args: [nx.Bytes(VEC_BYTES)] });
    const got = echoed.value.kind === 'Bytes' ? echoed.value.value : new Uint8Array();
    const ok = bytesEqual(got, VEC_BYTES) && VEC_BYTES.length === 4 * VEC.length;
    report('knn_bytes', ok, `${toHex(VEC_BYTES)} -> ${toHex(got)}`);
    failures += ok ? 0 : 1;
  } catch (exc) {
    report('knn_bytes', false, describeError(exc));
    failures += 1;
  }

  // 4. error -- a deliberately broken CYPHER yields a typed server error
  //    (not a transport crash), and the same connection stays usable.
  try {
    try {
      await cypherRows(authed, 'MATCH (n RETURN', {});
      report('error', false, 'expected a server error, got a result');
      failures += 1;
    } catch (exc) {
      const pong = await authed.execute({ command: 'PING', args: [] });
      const alive = pong.value.kind === 'Str' && pong.value.value === 'PONG';
      const excName = exc && exc.constructor ? exc.constructor.name : 'Error';
      report('error', alive, `raised ${excName}; connection alive=${alive}`);
      failures += alive ? 0 : 1;
    }
  } finally {
    await authed.close();
  }

  return failures ? 1 : 0;
}

main()
  .then((code) => {
    process.exit(code);
  })
  .catch((exc) => {
    console.error(exc);
    process.exit(1);
  });
