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

    // Create nodes
    echo "--- Creating Nodes ---\n";
    $alice = $client->createNode(
        labels: ['Person'],
        properties: [
            'name' => 'Alice',
            'age' => 28,
            'city' => 'San Francisco'
        ]
    );
    echo "Created: {$alice->properties['name']} (ID: {$alice->id})\n";

    $bob = $client->createNode(
        labels: ['Person'],
        properties: [
            'name' => 'Bob',
            'age' => 32,
            'city' => 'New York'
        ]
    );
    echo "Created: {$bob->properties['name']} (ID: {$bob->id})\n";

    // Create relationship
    echo "\n--- Creating Relationship ---\n";
    $rel = $client->createRelationship(
        startNode: $alice->id,
        endNode: $bob->id,
        type: 'KNOWS',
        properties: [
            'since' => '2020',
            'strength' => 0.8
        ]
    );
    echo "Created: {$alice->properties['name']} -[{$rel->type}]-> {$bob->properties['name']}\n";

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

    // Get node by ID
    echo "\n--- Reading Node ---\n";
    $node = $client->getNode($alice->id);
    echo "Retrieved: {$node->properties['name']}\n";

    // Update node
    echo "\n--- Updating Node ---\n";
    $updated = $client->updateNode(
        id: $alice->id,
        properties: [
            'age' => 29,
            'city' => 'Los Angeles'
        ]
    );
    echo "Updated: {$updated->properties['name']} is now {$updated->properties['age']} years old and lives in {$updated->properties['city']}\n";

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

    $client->deleteNode($alice->id);
    echo "✓ Deleted Alice\n";

    $client->deleteNode($bob->id);
    echo "✓ Deleted Bob\n";

    echo "\n✓ Example completed successfully\n";

} catch (NexusApiException $e) {
    echo "API Error: HTTP {$e->statusCode}: {$e->responseBody}\n";
} catch (Exception $e) {
    echo "Error: {$e->getMessage()}\n";
}
