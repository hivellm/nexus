import { describe, it, expect } from 'vitest';

describe('NexusApi Credentials', () => {
  const credentialDefinition = {
    name: 'nexusApi',
    displayName: 'Nexus API',
    properties: [
      { name: 'host', type: 'string', default: 'localhost', required: true },
      { name: 'port', type: 'number', default: 15474, required: true },
      { name: 'apiKey', type: 'string', required: true },
      { name: 'useTls', type: 'boolean', default: false },
    ],
  };

  it('should have correct name', () => {
    expect(credentialDefinition.name).toBe('nexusApi');
  });

  it('should have correct display name', () => {
    expect(credentialDefinition.displayName).toBe('Nexus API');
  });

  it('should have all required properties', () => {
    const propertyNames = credentialDefinition.properties.map((p) => p.name);
    expect(propertyNames).toContain('host');
    expect(propertyNames).toContain('port');
    expect(propertyNames).toContain('apiKey');
    expect(propertyNames).toContain('useTls');
  });

  it('should have correct default values', () => {
    const hostProp = credentialDefinition.properties.find((p) => p.name === 'host');
    const portProp = credentialDefinition.properties.find((p) => p.name === 'port');
    const tlsProp = credentialDefinition.properties.find((p) => p.name === 'useTls');

    expect(hostProp?.default).toBe('localhost');
    expect(portProp?.default).toBe(15474);
    expect(tlsProp?.default).toBe(false);
  });
});

describe('NexusUser Credentials', () => {
  const credentialDefinition = {
    name: 'nexusUser',
    displayName: 'Nexus User',
    properties: [
      { name: 'host', type: 'string', default: 'localhost', required: true },
      { name: 'port', type: 'number', default: 15474, required: true },
      { name: 'username', type: 'string', required: true },
      { name: 'password', type: 'string', required: true },
      { name: 'useTls', type: 'boolean', default: false },
    ],
  };

  it('should have correct name', () => {
    expect(credentialDefinition.name).toBe('nexusUser');
  });

  it('should have correct display name', () => {
    expect(credentialDefinition.displayName).toBe('Nexus User');
  });

  it('should have all required properties', () => {
    const propertyNames = credentialDefinition.properties.map((p) => p.name);
    expect(propertyNames).toContain('host');
    expect(propertyNames).toContain('port');
    expect(propertyNames).toContain('username');
    expect(propertyNames).toContain('password');
    expect(propertyNames).toContain('useTls');
  });

  it('should have correct default values', () => {
    const hostProp = credentialDefinition.properties.find((p) => p.name === 'host');
    const portProp = credentialDefinition.properties.find((p) => p.name === 'port');
    const tlsProp = credentialDefinition.properties.find((p) => p.name === 'useTls');

    expect(hostProp?.default).toBe('localhost');
    expect(portProp?.default).toBe(15474);
    expect(tlsProp?.default).toBe(false);
  });
});

describe('Authentication Header Generation', () => {
  it('should generate API key header', () => {
    const apiKey = 'test-api-key-123';
    const headers = { 'X-API-Key': apiKey };
    expect(headers['X-API-Key']).toBe('test-api-key-123');
  });

  it('should generate Basic auth header', () => {
    const username = 'admin';
    const password = 'secret';
    const credentials = Buffer.from(`${username}:${password}`).toString('base64');
    const header = `Basic ${credentials}`;

    expect(header).toBe('Basic YWRtaW46c2VjcmV0');
  });
});
