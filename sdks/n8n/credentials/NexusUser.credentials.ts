import type {
  IAuthenticateGeneric,
  ICredentialTestRequest,
  ICredentialType,
  INodeProperties,
} from 'n8n-workflow';

export class NexusUser implements ICredentialType {
  name = 'nexusUser';
  displayName = 'Nexus User';
  documentationUrl = 'https://github.com/hivellm/nexus/tree/main/sdks/n8n';

  properties: INodeProperties[] = [
    {
      displayName: 'Host',
      name: 'host',
      type: 'string',
      default: 'localhost',
      required: true,
      description: 'Nexus server hostname or IP address',
    },
    {
      displayName: 'Port',
      name: 'port',
      type: 'number',
      default: 15474,
      required: true,
      description: 'Nexus server port',
    },
    {
      displayName: 'Username',
      name: 'username',
      type: 'string',
      default: '',
      required: true,
      description: 'Nexus username',
    },
    {
      displayName: 'Password',
      name: 'password',
      type: 'string',
      typeOptions: {
        password: true,
      },
      default: '',
      required: true,
      description: 'Nexus password',
    },
    {
      displayName: 'Use HTTPS',
      name: 'useTls',
      type: 'boolean',
      default: false,
      description: 'Whether to use HTTPS for secure connections',
    },
  ];

  authenticate: IAuthenticateGeneric = {
    type: 'generic',
    properties: {
      headers: {
        Authorization:
          '=Basic {{Buffer.from($credentials.username + ":" + $credentials.password).toString("base64")}}',
      },
    },
  };

  test: ICredentialTestRequest = {
    request: {
      baseURL: '={{$credentials.useTls ? "https" : "http"}}://{{$credentials.host}}:{{$credentials.port}}',
      url: '/query',
      method: 'POST',
      body: {
        cypher: 'RETURN 1 AS test',
        params: {},
      },
    },
  };
}
