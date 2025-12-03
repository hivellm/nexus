# Installation Guide - n8n-nodes-nexus

This guide covers different installation methods for the Nexus n8n node.

## Prerequisites

- n8n installed (version 1.0.0 or higher)
- Node.js 18.x or higher
- A running Nexus Graph Database instance

## Installation Methods

### Method 1: Via n8n UI (Recommended)

This is the easiest method for most users.

1. Open your n8n instance
2. Navigate to **Settings** → **Community Nodes**
3. Click on **Install a community node**
4. Enter the package name: `@hivellm/n8n-nodes-nexus`
5. Click **Install**
6. Wait for the installation to complete
7. Restart n8n to activate the node

### Method 2: Via npm (Self-Hosted n8n)

If you're running n8n via npm:

```bash
# Navigate to your n8n installation directory
cd ~/.n8n

# Install the node package
npm install @hivellm/n8n-nodes-nexus

# Restart n8n
n8n start
```

### Method 3: Via Docker

If you're running n8n in Docker, add the package to your Docker environment:

#### Option A: Using Environment Variable

```bash
docker run -it --rm \
  --name n8n \
  -p 5678:5678 \
  -e N8N_COMMUNITY_PACKAGES="@hivellm/n8n-nodes-nexus" \
  -v ~/.n8n:/home/node/.n8n \
  n8nio/n8n
```

#### Option B: Using docker-compose.yml

```yaml
version: '3'

services:
  n8n:
    image: n8nio/n8n
    ports:
      - "5678:5678"
    environment:
      - N8N_COMMUNITY_PACKAGES=@hivellm/n8n-nodes-nexus
    volumes:
      - ~/.n8n:/home/node/.n8n
```

### Method 4: Manual Installation (Development)

For development or testing purposes:

```bash
# Clone the repository
git clone https://github.com/hivellm/nexus.git
cd nexus/sdks/n8n

# Install dependencies
npm install

# Build the node
npm run build

# Link globally
npm link

# In your n8n installation
cd ~/.n8n
npm link @hivellm/n8n-nodes-nexus

# Restart n8n
n8n start
```

## Verification

After installation, verify the node is available:

1. Create a new workflow in n8n
2. Click the **+** button to add a node
3. Search for "Nexus"
4. You should see the **Nexus** node in the results

## Setting Up Credentials

### Step 1: Create Nexus Credentials

1. In your workflow, add a Nexus node
2. Click on **Credential to connect with**
3. Click **Create New**
4. Choose authentication method:
   - **Nexus API** (API Key authentication)
   - **Nexus User** (Username/Password authentication)

### Step 2: Configure API Key Authentication

If using API key:

1. **Host**: Enter your Nexus server hostname (e.g., `localhost` or `nexus.example.com`)
2. **Port**: Enter the port (default: `15474`)
3. **API Key**: Enter your Nexus API key
4. **Use HTTPS**: Enable if using SSL/TLS
5. Click **Save**

### Step 3: Configure User/Password Authentication

If using username/password:

1. **Host**: Enter your Nexus server hostname
2. **Port**: Enter the port (default: `15474`)
3. **Username**: Enter your Nexus username
4. **Password**: Enter your Nexus password
5. **Use HTTPS**: Enable if using SSL/TLS
6. Click **Save**

### Step 4: Test Connection

1. After saving credentials, test the connection
2. Add a Nexus node to your workflow
3. Select **Execute Cypher** operation
4. Enter a simple query: `RETURN 1 as test`
5. Execute the node
6. Verify you get a successful result

## Troubleshooting

### Node Not Appearing

**Problem**: Nexus node doesn't appear in the node list after installation.

**Solutions**:
1. Restart n8n completely
2. Clear browser cache and reload
3. Verify the package is installed: `npm list @hivellm/n8n-nodes-nexus`
4. Check n8n logs for errors

### Connection Errors

**Problem**: Cannot connect to Nexus server.

**Solutions**:
1. Verify Nexus server is running
2. Check host and port are correct
3. Verify firewall allows connections
4. Test connection with curl:
   ```bash
   curl http://localhost:15474/health
   ```

### Authentication Errors

**Problem**: Authentication fails with valid credentials.

**Solutions**:
1. Verify API key is correct (check for copy/paste errors)
2. Ensure username/password are correct
3. Check if user has proper permissions
4. Verify authentication is enabled in Nexus config

### SSL/TLS Errors

**Problem**: HTTPS connection fails.

**Solutions**:
1. Verify certificate is valid
2. For self-signed certificates, you may need to:
   - Add certificate to system trust store
   - Or disable SSL verification (development only)

### Permission Errors

**Problem**: Operations fail with permission denied.

**Solutions**:
1. Verify user has proper permissions in Nexus
2. Check RBAC settings
3. Ensure API key has required permissions

## Updating

### Via n8n UI

1. Go to **Settings** → **Community Nodes**
2. Find `@hivellm/n8n-nodes-nexus`
3. Click **Update** if available

### Via npm

```bash
cd ~/.n8n
npm update @hivellm/n8n-nodes-nexus
```

### Via Docker

Pull the latest image and restart:

```bash
docker pull n8nio/n8n
docker restart n8n
```

## Uninstallation

### Via n8n UI

1. Go to **Settings** → **Community Nodes**
2. Find `@hivellm/n8n-nodes-nexus`
3. Click **Uninstall**
4. Restart n8n

### Via npm

```bash
cd ~/.n8n
npm uninstall @hivellm/n8n-nodes-nexus
```

## Getting Help

- **Documentation**: Check the [README](README.md) for usage examples
- **Issues**: Report bugs at [GitHub Issues](https://github.com/hivellm/nexus/issues)
- **Community**: Join the discussion in n8n Community forum
- **Nexus Docs**: [Nexus Documentation](https://github.com/hivellm/nexus)

## Next Steps

After successful installation:

1. Review the [workflow examples](examples/)
2. Check the [README](README.md) for operation details
3. Start building your graph automation workflows!
