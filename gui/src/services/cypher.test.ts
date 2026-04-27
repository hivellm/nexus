/**
 * `sanitizeCypher` tests. The Run button passes the editor body
 * through this helper before sending it to `/cypher` so leading
 * `//` comments and trailing semicolons no longer trigger the
 * Nexus parser's `Query must contain at least one clause` error.
 */
import { describe, it, expect } from 'vitest';
import { sanitizeCypher } from './cypher';

describe('sanitizeCypher', () => {
  it('strips a leading line comment and keeps the clause', () => {
    const out = sanitizeCypher(`// hello\nMATCH (n) RETURN n`);
    expect(out).toBe('MATCH (n) RETURN n');
  });

  it('strips block comments anywhere', () => {
    const out = sanitizeCypher(`/* doc */\nMATCH (n)\n/* mid */ RETURN n`);
    expect(out).toContain('MATCH (n)');
    expect(out).toContain('RETURN n');
    expect(out).not.toContain('/*');
  });

  it('strips `--` line comments', () => {
    const out = sanitizeCypher(`-- header\nMATCH (n) RETURN n`);
    expect(out).toBe('MATCH (n) RETURN n');
  });

  it('drops trailing semicolon', () => {
    expect(sanitizeCypher('MATCH (n) RETURN n;')).toBe('MATCH (n) RETURN n');
  });

  it('keeps interior blank lines after the first clause', () => {
    const out = sanitizeCypher(`MATCH (n)\n\nRETURN n`);
    expect(out).toContain('MATCH (n)');
    expect(out).toContain('RETURN n');
  });

  it('returns empty string when input is comments only', () => {
    expect(sanitizeCypher(`// only\n// comments`)).toBe('');
  });
});
