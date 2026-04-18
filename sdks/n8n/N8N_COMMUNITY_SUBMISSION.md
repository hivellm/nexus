# n8n Community Submission Guide

This guide walks you through submitting the Nexus node to the n8n community.

## Prerequisites

Before submitting, ensure:

- ✅ Package is published to npm
- ✅ All tests are passing
- ✅ Documentation is complete
- ✅ Examples are working
- ✅ README follows n8n standards
- ✅ Icon is provided (nexus.svg)
- ✅ License is specified (MIT)
- ✅ Version follows semver

## Step-by-Step Submission Process

### Step 1: Publish to npm

The package must be available on npm before submission.

```bash
# Make sure you're logged in to npm
npm login

# Verify package.json settings
# - name: @hivellm/n8n-nodes-nexus
# - version: 0.11.0
# - n8n.nodes: ["dist/nodes/Nexus/Nexus.node.js"]
# - n8n.credentials: ["dist/credentials/NexusApi.credentials.js", ...]

# Build the project
npm run build

# Publish to npm
npm publish --access public
```

Verify publication:
```bash
npm view @hivellm/n8n-nodes-nexus
```

### Step 2: Test Installation

Before submitting, test that users can install your node:

```bash
# Create a test n8n instance
mkdir ~/n8n-test
cd ~/n8n-test

# Install n8n
npm install n8n

# Install your node
npm install @hivellm/n8n-nodes-nexus

# Start n8n
npx n8n start

# Verify the node appears in the UI
```

### Step 3: Prepare Submission Materials

#### A. Package Information

Collect the following information:
- **Package name**: `@hivellm/n8n-nodes-nexus`
- **npm URL**: https://www.npmjs.com/package/@hivellm/n8n-nodes-nexus
- **Version**: 0.11.0
- **Category**: Database
- **Description**: Integration with Nexus Graph Database for graph operations
- **Author**: HiveLLM Team
- **License**: MIT
- **Repository**: https://github.com/hivellm/nexus

#### B. Screenshots

Prepare screenshots showing:
1. Node in the node panel
2. Node configuration interface
3. Credential setup
4. Example workflow
5. Execution results

Save in `screenshots/` directory:
```
screenshots/
  ├── 01-node-panel.png
  ├── 02-configuration.png
  ├── 03-credentials.png
  ├── 04-workflow-example.png
  └── 05-results.png
```

#### C. Demo Workflow

Create a shareable demo workflow:

1. Create a simple workflow in n8n
2. Use basic operations (Create Node, Execute Query, etc.)
3. Add helpful notes explaining each step
4. Export the workflow as JSON
5. Save as `examples/demo-workflow.json`

### Step 4: Submit to n8n Community

#### Option A: n8n Community Platform (Recommended)

1. Visit [n8n Community](https://community.n8n.io/)
2. Create an account if needed
3. Navigate to **Community Nodes** section
4. Click **Submit a Node**
5. Fill out the submission form:

   **Basic Information**:
   - Node Name: Nexus
   - Package Name: @hivellm/n8n-nodes-nexus
   - npm URL: https://www.npmjs.com/package/@hivellm/n8n-nodes-nexus
   - Category: Database
   - Tags: graph, database, cypher, neo4j, nexus

   **Description**:
   ```
   Official n8n integration for Nexus Graph Database. Execute Cypher queries,
   manage nodes and relationships, and perform graph operations directly from
   your n8n workflows. Supports batch operations, graph algorithms, and
   comprehensive schema management.
   ```

   **Features**:
   - Execute Cypher queries with parameter binding
   - Full CRUD operations for nodes and relationships
   - Batch operations for bulk data processing
   - Graph algorithms (shortest path)
   - Schema management and inspection
   - API key and user/password authentication

   **Links**:
   - GitHub: https://github.com/hivellm/nexus/tree/main/sdks/n8n
   - Documentation: https://github.com/hivellm/nexus/blob/main/sdks/n8n/README.md
   - Examples: https://github.com/hivellm/nexus/tree/main/sdks/n8n/examples
   - License: MIT

   **Screenshots**: Upload prepared screenshots

   **Demo Workflow**: Upload demo-workflow.json

6. Submit for review

#### Option B: GitHub Discussion

If the platform submission isn't available:

1. Go to [n8n GitHub Discussions](https://github.com/n8n-io/n8n/discussions)
2. Create a new discussion in "Community Nodes"
3. Use this template:

```markdown
# [Community Node] Nexus Graph Database

## Package Information
- **Name**: @hivellm/n8n-nodes-nexus
- **Version**: 0.11.0
- **npm**: https://www.npmjs.com/package/@hivellm/n8n-nodes-nexus
- **GitHub**: https://github.com/hivellm/nexus/tree/main/sdks/n8n

## Description

Official n8n integration for Nexus Graph Database, enabling graph operations
and Cypher query execution directly from n8n workflows.

## Features

- ✅ Execute Cypher queries with parameters
- ✅ Complete CRUD for nodes and relationships
- ✅ Batch operations (nodes & relationships)
- ✅ Graph algorithms (shortest path)
- ✅ Schema management
- ✅ API Key & User/Password auth
- ✅ 24 unit tests with 80%+ coverage
- ✅ Comprehensive documentation

## Operations Supported (16)

**Query**: Execute Cypher
**Nodes**: Create, Read, Update, Delete, Find
**Relationships**: Create, Read, Update, Delete
**Batch**: Create Nodes, Create Relationships
**Schema**: List Labels, List Types, Get Schema
**Algorithms**: Shortest Path

## Installation

\`\`\`bash
npm install @hivellm/n8n-nodes-nexus
\`\`\`

Or via n8n UI: Settings → Community Nodes → Install

## Documentation

- [README](https://github.com/hivellm/nexus/blob/main/sdks/n8n/README.md)
- [Installation Guide](https://github.com/hivellm/nexus/blob/main/sdks/n8n/INSTALLATION.md)
- [Contributing](https://github.com/hivellm/nexus/blob/main/sdks/n8n/CONTRIBUTING.md)
- [Workflow Examples](https://github.com/hivellm/nexus/tree/main/sdks/n8n/examples)

## Testing

All tests passing:
\`\`\`bash
npm test
✓ 24 tests passing
\`\`\`

## Screenshots

[Attach screenshots here]

## Demo Workflow

[Attach workflow JSON]

## License

MIT

---

Ready for community review! 🚀
```

### Step 5: n8n Documentation

After approval, add to n8n documentation:

1. Fork [n8n-docs](https://github.com/n8n-io/n8n-docs)
2. Add node documentation:
   - Create `docs/integrations/builtin/app-nodes/n8n-nodes-nexus.md`
   - Follow n8n documentation format
3. Add to nodes list in sidebar
4. Create pull request

### Step 6: Promote Your Node

After submission:

1. **Social Media**
   - Tweet about the release
   - Post in relevant communities (graph databases, workflow automation)
   - Share on LinkedIn

2. **n8n Community**
   - Create a "Show and Tell" post with example workflows
   - Help users with questions
   - Share use cases

3. **Documentation**
   - Create blog post about integration
   - Record tutorial video
   - Create more advanced workflow examples

## Post-Submission Checklist

- [ ] Node submitted to n8n community
- [ ] npm package published and accessible
- [ ] GitHub repository has releases tagged
- [ ] Documentation is complete
- [ ] Demo workflow is available
- [ ] Screenshots are added
- [ ] Social media announcement posted
- [ ] Ready to respond to community feedback

## Responding to Feedback

When you receive feedback:

1. **Acknowledge promptly**
   - Thank reviewers
   - Confirm you understand the feedback

2. **Address issues quickly**
   - Fix bugs
   - Improve documentation
   - Update examples

3. **Communicate changes**
   - Update the submission
   - Notify reviewers
   - Update changelog

## Maintenance Plan

After acceptance:

1. **Regular Updates**
   - Fix bugs promptly
   - Add requested features
   - Keep dependencies updated
   - Maintain compatibility with n8n versions

2. **Community Support**
   - Answer questions
   - Help with issues
   - Accept contributions

3. **Documentation**
   - Keep README updated
   - Add more examples
   - Create tutorial content

## Success Metrics

Track these metrics:
- npm downloads per week
- GitHub stars
- Community forum discussions
- Feature requests
- Bug reports (and resolution time)
- User feedback

## Resources

- [n8n Community Nodes Docs](https://docs.n8n.io/integrations/community-nodes/)
- [Creating Nodes Guide](https://docs.n8n.io/integrations/creating-nodes/)
- [n8n Community Forum](https://community.n8n.io/)
- [n8n GitHub](https://github.com/n8n-io/n8n)

## Need Help?

- Review existing community node submissions
- Ask in n8n Community forums
- Check n8n Discord server
- Refer to n8n documentation

Good luck with your submission! 🎉
