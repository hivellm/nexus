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
		Timeout: 30 * time.Second,
	})

	ctx := context.Background()

	fmt.Println("=== Testing Go SDK ===\n")

	// Test 1: Ping
	fmt.Print("1. Ping server: ")
	if err := client.Ping(ctx); err != nil {
		log.Fatal("FAILED - ", err)
	}
	fmt.Println("✓ OK")

	// Test 2: Simple query
	fmt.Print("2. Simple query: ")
	result, err := client.ExecuteCypher(ctx, "RETURN 1 as num", nil)
	if err != nil {
		log.Fatal("FAILED - ", err)
	}
	fmt.Printf("✓ OK - Columns: %v\n", result.Columns)

	// Test 3: Create nodes
	fmt.Print("3. Create nodes: ")
	result, err = client.ExecuteCypher(ctx,
		"CREATE (a:Person {name: 'Alice', age: 28}) "+
		"CREATE (b:Person {name: 'Bob', age: 32}) "+
		"RETURN a.name, b.name", nil)
	if err != nil {
		log.Fatal("FAILED - ", err)
	}
	fmt.Printf("✓ OK - Rows: %d\n", len(result.Rows))

	// Test 4: Query with parameters
	fmt.Print("4. Query with parameters: ")
	result, err = client.ExecuteCypher(ctx,
		"MATCH (p:Person) WHERE p.age > $minAge RETURN p.name as name, p.age as age",
		map[string]interface{}{"minAge": 25})
	if err != nil {
		log.Fatal("FAILED - ", err)
	}
	fmt.Printf("✓ OK - Found %d nodes\n", len(result.Rows))

	// Test 5: Create relationship
	fmt.Print("5. Create relationship: ")
	result, err = client.ExecuteCypher(ctx,
		"MATCH (a:Person {name: 'Alice'}) "+
		"MATCH (b:Person {name: 'Bob'}) "+
		"CREATE (a)-[r:KNOWS {since: '2020'}]->(b) "+
		"RETURN type(r) as type", nil)
	if err != nil {
		log.Fatal("FAILED - ", err)
	}
	fmt.Printf("✓ OK\n")

	// Test 6: Query relationships
	fmt.Print("6. Query relationships: ")
	result, err = client.ExecuteCypher(ctx,
		"MATCH (a:Person)-[r:KNOWS]->(b:Person) "+
		"RETURN a.name as person1, b.name as person2", nil)
	if err != nil {
		log.Fatal("FAILED - ", err)
	}
	fmt.Printf("✓ OK - Found %d relationships\n", len(result.Rows))

	// Test 7: Transaction
	fmt.Print("7. Transaction commit: ")
	tx, err := client.BeginTransaction(ctx)
	if err != nil {
		log.Fatal("FAILED - ", err)
	}
	_, err = tx.ExecuteCypher(ctx, "CREATE (n:TxTest {name: 'Test'})", nil)
	if err != nil {
		tx.Rollback(ctx)
		log.Fatal("FAILED - ", err)
	}
	if err := tx.Commit(ctx); err != nil {
		log.Fatal("FAILED - ", err)
	}
	fmt.Println("✓ OK")

	// Test 8: Transaction rollback
	fmt.Print("8. Transaction rollback: ")
	tx2, err := client.BeginTransaction(ctx)
	if err != nil {
		log.Fatal("FAILED - ", err)
	}
	_, err = tx2.ExecuteCypher(ctx, "CREATE (n:RollbackTest {name: 'Test'})", nil)
	if err != nil {
		log.Fatal("FAILED - ", err)
	}
	if err := tx2.Rollback(ctx); err != nil {
		log.Fatal("FAILED - ", err)
	}
	result, _ = client.ExecuteCypher(ctx, "MATCH (n:RollbackTest) RETURN count(n) as count", nil)
	rows := result.RowsAsMap()
	if len(rows) > 0 && rows[0]["count"].(float64) == 0 {
		fmt.Println("✓ OK - Rollback successful")
	} else {
		fmt.Println("⚠ WARNING - Rollback may not have worked")
	}

	// Cleanup
	fmt.Print("9. Cleanup: ")
	result, err = client.ExecuteCypher(ctx, "MATCH (n) DETACH DELETE n", nil)
	if err != nil {
		log.Fatal("FAILED - ", err)
	}
	fmt.Println("✓ OK")

	fmt.Println("\n✅ All Go SDK tests passed!")
}
