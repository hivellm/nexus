using Nexus.SDK;

var config = new NexusClientConfig
{
    BaseUrl = "http://localhost:15474",
    Timeout = TimeSpan.FromSeconds(30)
};

using var client = new NexusClient(config);

try
{
    Console.WriteLine("=== Testing C# SDK ===\n");

    // Ping
    Console.Write("1. Ping server: ");
    await client.PingAsync();
    Console.WriteLine("OK");

    // Simple query
    Console.Write("2. Simple query: ");
    var result = await client.ExecuteCypherAsync("RETURN 1 as num");
    Console.WriteLine($"OK - Columns: {string.Join(", ", result.Columns)}");

    // Create nodes
    Console.Write("3. Create nodes: ");
    result = await client.ExecuteCypherAsync(
        "CREATE (a:Person {name: 'Alice', age: 28}) " +
        "CREATE (b:Person {name: 'Bob', age: 32}) " +
        "RETURN a, b");
    Console.WriteLine($"OK - Created {result.Stats?.NodesCreated} nodes");

    // Query with parameters
    Console.Write("4. Query nodes: ");
    result = await client.ExecuteCypherAsync(
        "MATCH (p:Person) WHERE p.age > $minAge RETURN p.name as name, p.age as age",
        new Dictionary<string, object?> { ["minAge"] = 25 });
    Console.WriteLine($"OK - Found {result.Rows.Count} nodes");

    // Cleanup
    Console.Write("5. Cleanup: ");
    result = await client.ExecuteCypherAsync("MATCH (n:Person) DETACH DELETE n");
    Console.WriteLine($"OK - Deleted {result.Stats?.NodesDeleted} nodes");

    Console.WriteLine("\n[SUCCESS] All C# SDK tests passed!");
}
catch (NexusApiException ex)
{
    Console.WriteLine($"\n[ERROR] API Error: HTTP {ex.StatusCode}: {ex.ResponseBody}");
    Environment.Exit(1);
}
catch (Exception ex)
{
    Console.WriteLine($"\n[ERROR] Error: {ex.Message}");
    Environment.Exit(1);
}
