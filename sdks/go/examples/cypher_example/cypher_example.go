package main

import (
	"context"
	"fmt"
	"log"
	"time"

	nexus "github.com/hivellm/nexus-go"
)

func main() {
	// Create client
	client := nexus.NewClient(nexus.Config{
		BaseURL: "http://localhost:15474",
		Timeout: 30 * time.Second,
	})

	ctx := context.Background()

	// Check connection
	fmt.Println("Connecting to Nexus...")
	if err := client.Ping(ctx); err != nil {
		log.Fatal("Failed to connect:", err)
	}
	fmt.Println("✓ Connected successfully")

	// Create nodes using Cypher
	fmt.Println("--- Creating Nodes with Cypher ---")
	result, err := client.ExecuteCypher(ctx, `
		CREATE (a:Person {name: 'Alice', age: 28, city: 'San Francisco'})
		CREATE (b:Person {name: 'Bob', age: 32, city: 'New York'})
		RETURN a, b
	`, nil)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Created %d nodes\n", result.Stats.NodesCreated)

	// Create relationship
	fmt.Println("\n--- Creating Relationship ---")
	result, err = client.ExecuteCypher(ctx, `
		MATCH (a:Person {name: 'Alice'})
		MATCH (b:Person {name: 'Bob'})
		CREATE (a)-[r:KNOWS {since: '2020', strength: 0.8}]->(b)
		RETURN r
	`, nil)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Created %d relationships\n", result.Stats.RelationshipsCreated)

	// Query data
	fmt.Println("\n--- Querying Data ---")
	result, err = client.ExecuteCypher(ctx, `
		MATCH (p:Person)
		WHERE p.age > $minAge
		RETURN p.name as name, p.age as age, p.city as city
		ORDER BY p.age
	`, map[string]interface{}{
		"minAge": 25,
	})
	if err != nil {
		log.Fatal(err)
	}

	fmt.Printf("Found %d people older than 25:\n", len(result.Rows))
	for _, row := range result.RowsAsMap() {
		fmt.Printf("  - %v, %v years old, from %v\n", row["name"], row["age"], row["city"])
	}
	fmt.Printf("Query took %.2fms\n", result.Stats.ExecutionTimeMs)

	// Query with relationships
	fmt.Println("\n--- Querying Relationships ---")
	result, err = client.ExecuteCypher(ctx, `
		MATCH (a:Person)-[r:KNOWS]->(b:Person)
		RETURN a.name as person1, r.since as since, b.name as person2
	`, nil)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Printf("Found %d relationships:\n", len(result.Rows))
	for _, row := range result.RowsAsMap() {
		fmt.Printf("  %v knows %v since %v\n", row["person1"], row["person2"], row["since"])
	}

	// Cleanup
	fmt.Println("\n--- Cleanup ---")
	result, err = client.ExecuteCypher(ctx, `
		MATCH (n:Person)
		DETACH DELETE n
	`, nil)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Deleted %d nodes\n", result.Stats.NodesDeleted)

	fmt.Println("\n✓ Example completed successfully")
}
