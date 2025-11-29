<?php

require __DIR__ . '/vendor/autoload.php';

use Nexus\SDK\Config;
use Nexus\SDK\NexusClient;

$config = new Config(
    baseUrl: 'http://localhost:15474',
    timeout: 30
);

$client = new NexusClient($config);

try {
    echo "Testing Nexus PHP SDK...\n\n";

    // Ping
    echo "1. Ping server: ";
    $client->ping();
    echo "✓ OK\n";

    // Simple query
    echo "2. Simple query: ";
    $result = $client->executeCypher('RETURN 1 as num');
    echo "✓ OK - Columns: " . implode(', ', $result->columns) . "\n";

    // Create nodes
    echo "3. Create nodes: ";
    $result = $client->executeCypher(
        "CREATE (a:Person {name: 'Alice', age: 28}) " .
        "CREATE (b:Person {name: 'Bob', age: 32}) " .
        "RETURN a, b"
    );
    echo "✓ OK - Created " . $result->stats->nodesCreated . " nodes\n";

    // Query
    echo "4. Query nodes: ";
    $result = $client->executeCypher(
        'MATCH (p:Person) WHERE p.age > $minAge RETURN p.name as name, p.age as age',
        ['minAge' => 25]
    );
    echo "✓ OK - Found " . count($result->rows) . " nodes\n";

    // Cleanup
    echo "5. Cleanup: ";
    $result = $client->executeCypher('MATCH (n:Person) DETACH DELETE n');
    echo "✓ OK - Deleted " . $result->stats->nodesDeleted . " nodes\n";

    echo "\n✅ All PHP SDK tests passed!\n";

} catch (Exception $e) {
    echo "\n❌ Error: " . $e->getMessage() . "\n";
    exit(1);
}
