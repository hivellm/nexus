package main

import (
	"context"
	"fmt"
	"log"
	"time"

	nexus "github.com/hivellm/nexus-go"
)

func main() {
	client := nexus.NewClient(nexus.Config{
		BaseURL: "http://localhost:15474",
		APIKey:  "demo-api-key",
		Timeout: 30 * time.Second,
	})

	ctx := context.Background()

	// Check connection
	if err := client.Ping(ctx); err != nil {
		log.Fatal("Failed to connect:", err)
	}

	fmt.Println("=== Schema Management Example ===")

	// Create some sample data
	fmt.Println("--- Creating Sample Data ---")
	_, err := client.ExecuteCypher(ctx, `
		CREATE (alice:Person:Employee {name: 'Alice', email: 'alice@example.com', dept: 'Engineering'})
		CREATE (bob:Person:Employee {name: 'Bob', email: 'bob@example.com', dept: 'Sales'})
		CREATE (company:Company {name: 'TechCorp', founded: 2020})
		CREATE (product:Product {name: 'GraphDB', version: '1.0'})
		CREATE (alice)-[:WORKS_AT {since: 2020}]->(company)
		CREATE (bob)-[:WORKS_AT {since: 2021}]->(company)
		CREATE (alice)-[:MANAGES]->(product)
	`, nil)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ Sample data created")

	// Example 1: List all labels
	fmt.Println("--- Listing Labels ---")
	labels, err := client.ListLabels(ctx)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Printf("Found %d labels:\n", len(labels))
	for _, label := range labels {
		fmt.Printf("  - %s\n", label)
	}

	// Example 2: List all relationship types
	fmt.Println("\n--- Listing Relationship Types ---")
	types, err := client.ListRelationshipTypes(ctx)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Printf("Found %d relationship types:\n", len(types))
	for _, relType := range types {
		fmt.Printf("  - %s\n", relType)
	}

	// Example 3: Create indexes
	fmt.Println("\n--- Creating Indexes ---")

	// Index on Person.email
	err = client.CreateIndex(ctx, "person_email_idx", "Person", []string{"email"})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ Created index: person_email_idx")

	// Index on Person.name
	err = client.CreateIndex(ctx, "person_name_idx", "Person", []string{"name"})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ Created index: person_name_idx")

	// Index on Company.name
	err = client.CreateIndex(ctx, "company_name_idx", "Company", []string{"name"})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ Created index: company_name_idx")

	// Example 4: List indexes
	fmt.Println("\n--- Listing Indexes ---")
	indexes, err := client.ListIndexes(ctx)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Printf("Found %d indexes:\n", len(indexes))
	for _, idx := range indexes {
		fmt.Printf("  - %s: %s(%v) [%s]\n",
			idx.Name, idx.Label, idx.Properties, idx.Type)
	}

	// Example 5: Query schema information
	fmt.Println("\n--- Schema Information ---")

	// Count nodes by label
	result, err := client.ExecuteCypher(ctx, `
		MATCH (n)
		RETURN labels(n) as labels, count(n) as count
		ORDER BY count DESC
	`, nil)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Println("Nodes by label:")
	for _, row := range result.RowsAsMap() {
		labels := row["labels"].([]interface{})
		if len(labels) > 0 {
			fmt.Printf("  %v: %v nodes\n", labels, row["count"])
		}
	}

	// Count relationships by type
	result, err = client.ExecuteCypher(ctx, `
		MATCH ()-[r]->()
		RETURN type(r) as type, count(r) as count
		ORDER BY count DESC
	`, nil)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Println("\nRelationships by type:")
	for _, row := range result.RowsAsMap() {
		fmt.Printf("  %s: %v relationships\n", row["type"], row["count"])
	}

	// Example 6: Get property keys for a label
	result, err = client.ExecuteCypher(ctx, `
		MATCH (n:Person)
		UNWIND keys(n) as key
		RETURN DISTINCT key
		ORDER BY key
	`, nil)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Println("\nProperties for Person label:")
	for _, row := range result.RowsAsMap() {
		fmt.Printf("  - %s\n", row["key"])
	}

	// Example 7: Test index performance
	fmt.Println("\n--- Testing Index Performance ---")

	// Add more data for testing
	_, err = client.ExecuteCypher(ctx, `
		UNWIND range(1, 1000) as i
		CREATE (:Person {
			name: 'Person' + i,
			email: 'person' + i + '@example.com',
			age: i % 100
		})
	`, nil)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ Created 1000 test nodes")

	// Query with index
	start := time.Now()
	result, err = client.ExecuteCypher(ctx, `
		MATCH (p:Person {email: 'person500@example.com'})
		RETURN p.name
	`, nil)
	if err != nil {
		log.Fatal(err)
	}
	duration := time.Since(start)

	fmt.Printf("✓ Indexed query took %v\n", duration)
	if len(result.Rows) > 0 {
		fmt.Printf("  Found: %s\n", result.RowsAsMap()[0]["p.name"])
	}

	// Example 8: Delete an index
	fmt.Println("\n--- Deleting Index ---")

	err = client.DeleteIndex(ctx, "person_name_idx")
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ Deleted index: person_name_idx")

	// Verify deletion
	indexes, err = client.ListIndexes(ctx)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Printf("Remaining indexes: %d\n", len(indexes))
	for _, idx := range indexes {
		fmt.Printf("  - %s\n", idx.Name)
	}

	// Example 9: Schema constraints (via Cypher)
	fmt.Println("\n--- Schema Statistics ---")

	result, err = client.ExecuteCypher(ctx, `
		MATCH (n)
		RETURN count(DISTINCT labels(n)) as unique_label_combinations,
		       count(n) as total_nodes
	`, nil)
	if err != nil {
		log.Fatal(err)
	}

	if len(result.Rows) > 0 {
		row := result.RowsAsMap()[0]
		fmt.Printf("Total nodes: %v\n", row["total_nodes"])
		fmt.Printf("Unique label combinations: %v\n", row["unique_label_combinations"])
	}

	result, err = client.ExecuteCypher(ctx, `
		MATCH ()-[r]->()
		RETURN count(DISTINCT type(r)) as unique_types,
		       count(r) as total_relationships
	`, nil)
	if err != nil {
		log.Fatal(err)
	}

	if len(result.Rows) > 0 {
		row := result.RowsAsMap()[0]
		fmt.Printf("Total relationships: %v\n", row["total_relationships"])
		fmt.Printf("Unique relationship types: %v\n", row["unique_types"])
	}

	// Cleanup
	fmt.Println("\n--- Cleanup ---")

	// Delete remaining indexes
	indexes, _ = client.ListIndexes(ctx)
	for _, idx := range indexes {
		if err := client.DeleteIndex(ctx, idx.Name); err != nil {
			log.Printf("Warning: failed to delete index %s: %v", idx.Name, err)
		}
	}
	fmt.Println("✓ Deleted all indexes")

	// Delete all data
	_, err = client.ExecuteCypher(ctx, `
		MATCH (n)
		DETACH DELETE n
	`, nil)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ All data deleted")

	fmt.Println("\n✓ Schema management examples completed successfully")
}
