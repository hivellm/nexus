<?php

declare(strict_types=1);

require __DIR__ . '/../vendor/autoload.php';

use Nexus\SDK\Config;
use Nexus\SDK\NexusClient;
use Nexus\SDK\NexusApiException;

// Create client
$config = new Config(
    baseUrl: 'http://localhost:15474',
    apiKey: 'demo-api-key', // Replace with your API key
    timeout: 30
);

$client = new NexusClient($config);

try {
    // Check connection
    echo "Connecting to Nexus...\n";
    $client->ping();
    echo "✓ Connected successfully\n\n";

    // Create nodes — createNode now returns CreateNodeResponse {nodeId, message, error}.
    echo "--- Creating Nodes ---\n";
    $alice = $client->createNode(
        labels: ['Person'],
        properties: [
            'name' => 'Alice',
            'age' => 28,
            'city' => 'San Francisco'
        ]
    );
    echo "Created: Alice (ID: {$alice->nodeId})\n";

    $bob = $client->createNode(
        labels: ['Person'],
        properties: [
            'name' => 'Bob',
            'age' => 32,
            'city' => 'New York'
        ]
    );
    echo "Created: Bob (ID: {$bob->nodeId})\n";

    // Create relationship
    echo "\n--- Creating Relationship ---\n";
    $rel = $client->createRelationship(
        startNode: $alice->nodeId,
        endNode: $bob->nodeId,
        type: 'KNOWS',
        properties: [
            'since' => '2020',
            'strength' => 0.8
        ]
    );
    echo "Created: Alice -[{$rel->type}]-> Bob\n";

    // Query data
    echo "\n--- Querying Data ---\n";
    $result = $client->executeCypher(
        query: 'MATCH (p:Person) WHERE p.age > $minAge RETURN p.name as name, p.age as age, p.city as city ORDER BY p.age',
        parameters: ['minAge' => 25]
    );

    echo "Found " . count($result->rows) . " people older than 25:\n";
    foreach ($result->rows as $row) {
        echo "  - {$row['name']}, {$row['age']} years old, from {$row['city']}\n";
    }
    echo "Query took {$result->stats?->executionTimeMs}ms\n";

    // Get node by ID — returns GetNodeResponse {node, message, error}.
    echo "\n--- Reading Node ---\n";
    $resp = $client->getNode($alice->nodeId);
    if ($resp->node !== null) {
        echo "Retrieved: {$resp->node->properties['name']}\n";
    }

    // Update node — returns the raw {message, error} envelope.
    echo "\n--- Updating Node ---\n";
    $client->updateNode(
        id: $alice->nodeId,
        properties: [
            'age' => 29,
            'city' => 'Los Angeles'
        ]
    );
    echo "Updated Alice's age to 29 (Los Angeles)\n";

    // Query with relationships
    echo "\n--- Querying Relationships ---\n";
    $result = $client->executeCypher(
        'MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.name as person1, r.since as since, b.name as person2'
    );

    echo "Found " . count($result->rows) . " relationships:\n";
    foreach ($result->rows as $row) {
        echo "  {$row['person1']} knows {$row['person2']} since {$row['since']}\n";
    }

    // Cleanup
    echo "\n--- Cleanup ---\n";
    $client->deleteRelationship($rel->id);
    echo "✓ Deleted relationship\n";

    $client->deleteNode($alice->nodeId);
    echo "Deleted Alice\n";

    $client->deleteNode($bob->nodeId);
    echo "Deleted Bob\n";

    echo "\n✓ Example completed successfully\n";

} catch (NexusApiException $e) {
    echo "API Error: HTTP {$e->statusCode}: {$e->responseBody}\n";
} catch (Exception $e) {
    echo "Error: {$e->getMessage()}\n";
}
