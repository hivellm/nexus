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

	fmt.Println("=== Batch Operations Example ===")

	// Example 1: Batch create nodes
	fmt.Println("--- Batch Creating Nodes ---")
	start := time.Now()

	nodes, err := client.BatchCreateNodes(ctx, []struct {
		Labels     []string
		Properties map[string]interface{}
	}{
		{
			Labels: []string{"Person"},
			Properties: map[string]interface{}{
				"name": "Alice",
				"age":  28,
				"dept": "Engineering",
			},
		},
		{
			Labels: []string{"Person"},
			Properties: map[string]interface{}{
				"name": "Bob",
				"age":  32,
				"dept": "Engineering",
			},
		},
		{
			Labels: []string{"Person"},
			Properties: map[string]interface{}{
				"name": "Charlie",
				"age":  25,
				"dept": "Sales",
			},
		},
		{
			Labels: []string{"Person"},
			Properties: map[string]interface{}{
				"name": "Diana",
				"age":  30,
				"dept": "Marketing",
			},
		},
		{
			Labels: []string{"Person"},
			Properties: map[string]interface{}{
				"name": "Eve",
				"age":  27,
				"dept": "Engineering",
			},
		},
	})
	if err != nil {
		log.Fatal(err)
	}

	duration := time.Since(start)
	fmt.Printf("✓ Created %d nodes in %v\n", len(nodes), duration)

	for _, node := range nodes {
		fmt.Printf("  - %s (ID: %s)\n", node.Properties["name"], node.ID)
	}

	// Example 2: Batch create relationships
	fmt.Println("\n--- Batch Creating Relationships ---")
	start = time.Now()

	relationships, err := client.BatchCreateRelationships(ctx, []struct {
		StartNode  string
		EndNode    string
		Type       string
		Properties map[string]interface{}
	}{
		{
			StartNode: nodes[0].ID, // Alice
			EndNode:   nodes[1].ID, // Bob
			Type:      "WORKS_WITH",
			Properties: map[string]interface{}{
				"project": "GraphDB",
				"since":   "2020",
			},
		},
		{
			StartNode: nodes[0].ID, // Alice
			EndNode:   nodes[4].ID, // Eve
			Type:      "WORKS_WITH",
			Properties: map[string]interface{}{
				"project": "GraphDB",
				"since":   "2021",
			},
		},
		{
			StartNode: nodes[1].ID, // Bob
			EndNode:   nodes[4].ID, // Eve
			Type:      "WORKS_WITH",
			Properties: map[string]interface{}{
				"project": "GraphDB",
				"since":   "2021",
			},
		},
		{
			StartNode: nodes[2].ID, // Charlie
			EndNode:   nodes[3].ID, // Diana
			Type:      "WORKS_WITH",
			Properties: map[string]interface{}{
				"project": "Marketing Campaign",
				"since":   "2022",
			},
		},
	})
	if err != nil {
		log.Fatal(err)
	}

	duration = time.Since(start)
	fmt.Printf("✓ Created %d relationships in %v\n", len(relationships), duration)

	for _, rel := range relationships {
		fmt.Printf("  - %s [%s]\n", rel.ID, rel.Type)
	}

	// Example 3: Query the batch-created data
	fmt.Println("\n--- Querying Batch Data ---")
	result, err := client.ExecuteCypher(ctx, `
		MATCH (p:Person)
		RETURN p.dept as department, count(p) as count
		ORDER BY count DESC
	`, nil)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Println("People by department:")
	for _, row := range result.RowsAsMap() {
		fmt.Printf("  %s: %v people\n", row["department"], row["count"])
	}

	// Example 4: Query relationships
	result, err = client.ExecuteCypher(ctx, `
		MATCH (a:Person)-[r:WORKS_WITH]->(b:Person)
		RETURN a.name as person1, b.name as person2, r.project as project
		ORDER BY r.project, person1
	`, nil)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Println("\nWork relationships:")
	currentProject := ""
	for _, row := range result.RowsAsMap() {
		project := row["project"].(string)
		if project != currentProject {
			fmt.Printf("\n  Project: %s\n", project)
			currentProject = project
		}
		fmt.Printf("    %s works with %s\n", row["person1"], row["person2"])
	}

	// Example 5: Performance comparison
	fmt.Println("\n--- Performance Comparison ---")

	// Individual creates
	start = time.Now()
	for i := 0; i < 10; i++ {
		_, err := client.CreateNode(ctx, []string{"TestNode"}, map[string]interface{}{
			"name":  fmt.Sprintf("Individual%d", i),
			"index": i,
		})
		if err != nil {
			log.Fatal(err)
		}
	}
	individualDuration := time.Since(start)
	fmt.Printf("Individual creates (10 nodes): %v\n", individualDuration)

	// Batch create
	start = time.Now()
	batchNodes := make([]struct {
		Labels     []string
		Properties map[string]interface{}
	}, 10)
	for i := 0; i < 10; i++ {
		batchNodes[i] = struct {
			Labels     []string
			Properties map[string]interface{}
		}{
			Labels: []string{"TestNode"},
			Properties: map[string]interface{}{
				"name":  fmt.Sprintf("Batch%d", i),
				"index": i,
			},
		}
	}
	_, err = client.BatchCreateNodes(ctx, batchNodes)
	if err != nil {
		log.Fatal(err)
	}
	batchDuration := time.Since(start)
	fmt.Printf("Batch create (10 nodes): %v\n", batchDuration)

	speedup := float64(individualDuration) / float64(batchDuration)
	fmt.Printf("Speedup: %.2fx faster\n", speedup)

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

	fmt.Println("\n✓ Batch operations examples completed successfully")
}
