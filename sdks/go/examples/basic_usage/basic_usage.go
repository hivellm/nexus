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
		APIKey:  "demo-api-key", // Replace with your API key
		Timeout: 30 * time.Second,
	})

	ctx := context.Background()

	// Check connection
	fmt.Println("Connecting to Nexus...")
	if err := client.Ping(ctx); err != nil {
		log.Fatal("Failed to connect:", err)
	}
	fmt.Println("✓ Connected successfully")

	// Create nodes
	fmt.Println("\n--- Creating Nodes ---")
	alice, err := client.CreateNode(ctx, []string{"Person"}, map[string]interface{}{
		"name": "Alice",
		"age":  28,
		"city": "San Francisco",
	})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Created: %s (ID: %s)\n", alice.Properties["name"], alice.ID)

	bob, err := client.CreateNode(ctx, []string{"Person"}, map[string]interface{}{
		"name": "Bob",
		"age":  32,
		"city": "New York",
	})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Created: %s (ID: %s)\n", bob.Properties["name"], bob.ID)

	// Create relationship
	fmt.Println("\n--- Creating Relationship ---")
	rel, err := client.CreateRelationship(ctx, alice.ID, bob.ID, "KNOWS", map[string]interface{}{
		"since":    "2020",
		"strength": 0.8,
	})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Created: %s -[%s]-> %s\n", alice.Properties["name"], rel.Type, bob.Properties["name"])

	// Query data
	fmt.Println("\n--- Querying Data ---")
	result, err := client.ExecuteCypher(ctx, `
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
		fmt.Printf("  - %s, %v years old, from %s\n", row["name"], row["age"], row["city"])
	}
	fmt.Printf("Query took %.2fms\n", result.Stats.ExecutionTimeMs)

	// Get node by ID
	fmt.Println("\n--- Reading Node ---")
	node, err := client.GetNode(ctx, alice.ID)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Retrieved: %+v\n", node.Properties)

	// Update node
	fmt.Println("\n--- Updating Node ---")
	updated, err := client.UpdateNode(ctx, alice.ID, map[string]interface{}{
		"age":  29,
		"city": "Los Angeles",
	})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Updated: %s is now %v years old and lives in %s\n",
		updated.Properties["name"], updated.Properties["age"], updated.Properties["city"])

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
		fmt.Printf("  %s knows %s since %s\n", row["person1"], row["person2"], row["since"])
	}

	// Cleanup
	fmt.Println("\n--- Cleanup ---")
	if err := client.DeleteRelationship(ctx, rel.ID); err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ Deleted relationship")

	if err := client.DeleteNode(ctx, alice.ID); err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ Deleted Alice")

	if err := client.DeleteNode(ctx, bob.ID); err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ Deleted Bob")

	fmt.Println("\n✓ Example completed successfully")
}
