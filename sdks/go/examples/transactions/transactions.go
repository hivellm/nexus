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

	fmt.Println("=== Transaction Example ===")

	// Example 1: Successful transaction
	fmt.Println("--- Example 1: Successful Transaction ---")
	tx1, err := client.BeginTransaction(ctx)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ Transaction started")

	// Create nodes in transaction
	_, err = tx1.ExecuteCypher(ctx, `
		CREATE (a:Person {name: $name1, role: 'Developer'})
		CREATE (b:Person {name: $name2, role: 'Manager'})
		CREATE (a)-[:REPORTS_TO]->(b)
	`, map[string]interface{}{
		"name1": "Alice",
		"name2": "Bob",
	})
	if err != nil {
		tx1.Rollback(ctx)
		log.Fatal(err)
	}
	fmt.Println("✓ Created nodes and relationship")

	// Commit transaction
	if err := tx1.Commit(ctx); err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ Transaction committed")

	// Verify data was created
	result, err := client.ExecuteCypher(ctx, `
		MATCH (a:Person)-[r:REPORTS_TO]->(b:Person)
		RETURN a.name as employee, b.name as manager
	`, nil)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Printf("Found %d reporting relationships:\n", len(result.Rows))
	for _, row := range result.RowsAsMap() {
		fmt.Printf("  %s reports to %s\n", row["employee"], row["manager"])
	}

	// Example 2: Rollback transaction
	fmt.Println("\n--- Example 2: Rollback Transaction ---")
	tx2, err := client.BeginTransaction(ctx)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ Transaction started")

	// Create temporary nodes
	_, err = tx2.ExecuteCypher(ctx, `
		CREATE (p:Person {name: 'Temporary', status: 'temp'})
	`, nil)
	if err != nil {
		tx2.Rollback(ctx)
		log.Fatal(err)
	}
	fmt.Println("✓ Created temporary node")

	// Rollback transaction
	if err := tx2.Rollback(ctx); err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ Transaction rolled back")

	// Verify data was not persisted
	result, err = client.ExecuteCypher(ctx, `
		MATCH (p:Person {status: 'temp'})
		RETURN count(p) as count
	`, nil)
	if err != nil {
		log.Fatal(err)
	}

	count := int(result.RowsAsMap()[0]["count"].(float64))
	fmt.Printf("Temporary nodes found: %d (should be 0)\n", count)

	// Example 3: Multi-step transaction with error handling
	fmt.Println("\n--- Example 3: Complex Transaction ---")
	tx3, err := client.BeginTransaction(ctx)
	if err != nil {
		log.Fatal(err)
	}

	// Step 1: Create company
	_, err = tx3.ExecuteCypher(ctx, `
		CREATE (c:Company {name: $name, founded: $year})
	`, map[string]interface{}{
		"name": "TechCorp",
		"year": 2020,
	})
	if err != nil {
		tx3.Rollback(ctx)
		log.Fatal(err)
	}
	fmt.Println("✓ Step 1: Created company")

	// Step 2: Create employees
	_, err = tx3.ExecuteCypher(ctx, `
		MATCH (c:Company {name: $company})
		CREATE (e1:Person {name: 'John', role: 'CEO'})
		CREATE (e2:Person {name: 'Jane', role: 'CTO'})
		CREATE (e1)-[:WORKS_AT]->(c)
		CREATE (e2)-[:WORKS_AT]->(c)
	`, map[string]interface{}{
		"company": "TechCorp",
	})
	if err != nil {
		tx3.Rollback(ctx)
		log.Fatal(err)
	}
	fmt.Println("✓ Step 2: Added employees")

	// Step 3: Create org structure
	_, err = tx3.ExecuteCypher(ctx, `
		MATCH (ceo:Person {role: 'CEO'})
		MATCH (cto:Person {role: 'CTO'})
		CREATE (cto)-[:REPORTS_TO]->(ceo)
	`, nil)
	if err != nil {
		tx3.Rollback(ctx)
		log.Fatal(err)
	}
	fmt.Println("✓ Step 3: Created org structure")

	// Commit all changes
	if err := tx3.Commit(ctx); err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ All changes committed")

	// Verify final state
	result, err = client.ExecuteCypher(ctx, `
		MATCH (c:Company)<-[:WORKS_AT]-(e:Person)
		RETURN c.name as company, collect(e.name) as employees
	`, nil)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Println("Final company structure:")
	for _, row := range result.RowsAsMap() {
		employees := row["employees"].([]interface{})
		fmt.Printf("  %s has %d employees\n", row["company"], len(employees))
	}

	// Cleanup
	fmt.Println("\n--- Cleanup ---")
	_, err = client.ExecuteCypher(ctx, `
		MATCH (n)
		DETACH DELETE n
	`, nil)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("✓ All data deleted")

	fmt.Println("\n✓ Transaction examples completed successfully")
}
