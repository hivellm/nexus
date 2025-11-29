// Package nexus provides a Go client for the Nexus graph database.
package nexus

import (
	"fmt"
	"strings"
)

// QueryBuilder provides a fluent API for constructing Cypher queries.
type QueryBuilder struct {
	matchClauses   []string
	whereClauses   []string
	createClauses  []string
	setClauses     []string
	deleteClauses  []string
	returnClauses  []string
	orderByClauses []string
	skipValue      *int
	limitValue     *int
	parameters     map[string]interface{}
}

// NewQueryBuilder creates a new QueryBuilder instance.
func NewQueryBuilder() *QueryBuilder {
	return &QueryBuilder{
		matchClauses:   make([]string, 0),
		whereClauses:   make([]string, 0),
		createClauses:  make([]string, 0),
		setClauses:     make([]string, 0),
		deleteClauses:  make([]string, 0),
		returnClauses:  make([]string, 0),
		orderByClauses: make([]string, 0),
		parameters:     make(map[string]interface{}),
	}
}

// Match adds a MATCH clause to the query.
func (qb *QueryBuilder) Match(pattern string) *QueryBuilder {
	qb.matchClauses = append(qb.matchClauses, pattern)
	return qb
}

// OptionalMatch adds an OPTIONAL MATCH clause to the query.
func (qb *QueryBuilder) OptionalMatch(pattern string) *QueryBuilder {
	qb.matchClauses = append(qb.matchClauses, "OPTIONAL MATCH "+pattern)
	return qb
}

// Where adds a WHERE clause to the query.
func (qb *QueryBuilder) Where(condition string) *QueryBuilder {
	qb.whereClauses = append(qb.whereClauses, condition)
	return qb
}

// And adds an AND condition to the WHERE clause.
func (qb *QueryBuilder) And(condition string) *QueryBuilder {
	if len(qb.whereClauses) > 0 {
		qb.whereClauses[len(qb.whereClauses)-1] += " AND " + condition
	} else {
		qb.whereClauses = append(qb.whereClauses, condition)
	}
	return qb
}

// Or adds an OR condition to the WHERE clause.
func (qb *QueryBuilder) Or(condition string) *QueryBuilder {
	if len(qb.whereClauses) > 0 {
		qb.whereClauses[len(qb.whereClauses)-1] += " OR " + condition
	} else {
		qb.whereClauses = append(qb.whereClauses, condition)
	}
	return qb
}

// Create adds a CREATE clause to the query.
func (qb *QueryBuilder) Create(pattern string) *QueryBuilder {
	qb.createClauses = append(qb.createClauses, pattern)
	return qb
}

// Merge adds a MERGE clause to the query.
func (qb *QueryBuilder) Merge(pattern string) *QueryBuilder {
	qb.createClauses = append(qb.createClauses, "MERGE "+pattern)
	return qb
}

// Set adds a SET clause to the query.
func (qb *QueryBuilder) Set(assignment string) *QueryBuilder {
	qb.setClauses = append(qb.setClauses, assignment)
	return qb
}

// Delete adds a DELETE clause to the query.
func (qb *QueryBuilder) Delete(items string) *QueryBuilder {
	qb.deleteClauses = append(qb.deleteClauses, items)
	return qb
}

// DetachDelete adds a DETACH DELETE clause to the query.
func (qb *QueryBuilder) DetachDelete(items string) *QueryBuilder {
	qb.deleteClauses = append(qb.deleteClauses, "DETACH DELETE "+items)
	return qb
}

// Return adds a RETURN clause to the query.
func (qb *QueryBuilder) Return(items ...string) *QueryBuilder {
	qb.returnClauses = append(qb.returnClauses, items...)
	return qb
}

// ReturnDistinct adds a RETURN DISTINCT clause to the query.
func (qb *QueryBuilder) ReturnDistinct(items ...string) *QueryBuilder {
	if len(qb.returnClauses) == 0 {
		qb.returnClauses = append(qb.returnClauses, "DISTINCT "+strings.Join(items, ", "))
	} else {
		qb.returnClauses = append(qb.returnClauses, items...)
	}
	return qb
}

// OrderBy adds an ORDER BY clause to the query.
func (qb *QueryBuilder) OrderBy(items ...string) *QueryBuilder {
	qb.orderByClauses = append(qb.orderByClauses, items...)
	return qb
}

// OrderByDesc adds an ORDER BY ... DESC clause to the query.
func (qb *QueryBuilder) OrderByDesc(item string) *QueryBuilder {
	qb.orderByClauses = append(qb.orderByClauses, item+" DESC")
	return qb
}

// Skip adds a SKIP clause to the query.
func (qb *QueryBuilder) Skip(n int) *QueryBuilder {
	qb.skipValue = &n
	return qb
}

// Limit adds a LIMIT clause to the query.
func (qb *QueryBuilder) Limit(n int) *QueryBuilder {
	qb.limitValue = &n
	return qb
}

// WithParam adds a parameter to the query.
func (qb *QueryBuilder) WithParam(name string, value interface{}) *QueryBuilder {
	qb.parameters[name] = value
	return qb
}

// WithParams adds multiple parameters to the query.
func (qb *QueryBuilder) WithParams(params map[string]interface{}) *QueryBuilder {
	for k, v := range params {
		qb.parameters[k] = v
	}
	return qb
}

// Build constructs the final Cypher query string.
func (qb *QueryBuilder) Build() string {
	var parts []string

	// MATCH clauses
	for _, match := range qb.matchClauses {
		if strings.HasPrefix(match, "OPTIONAL MATCH") {
			parts = append(parts, match)
		} else {
			parts = append(parts, "MATCH "+match)
		}
	}

	// WHERE clauses
	if len(qb.whereClauses) > 0 {
		parts = append(parts, "WHERE "+strings.Join(qb.whereClauses, " AND "))
	}

	// CREATE/MERGE clauses
	for _, create := range qb.createClauses {
		if strings.HasPrefix(create, "MERGE") {
			parts = append(parts, create)
		} else {
			parts = append(parts, "CREATE "+create)
		}
	}

	// SET clauses
	if len(qb.setClauses) > 0 {
		parts = append(parts, "SET "+strings.Join(qb.setClauses, ", "))
	}

	// DELETE clauses
	for _, del := range qb.deleteClauses {
		if strings.HasPrefix(del, "DETACH DELETE") {
			parts = append(parts, del)
		} else {
			parts = append(parts, "DELETE "+del)
		}
	}

	// RETURN clause
	if len(qb.returnClauses) > 0 {
		returnStr := strings.Join(qb.returnClauses, ", ")
		if strings.HasPrefix(returnStr, "DISTINCT ") {
			parts = append(parts, "RETURN "+returnStr)
		} else {
			parts = append(parts, "RETURN "+returnStr)
		}
	}

	// ORDER BY clause
	if len(qb.orderByClauses) > 0 {
		parts = append(parts, "ORDER BY "+strings.Join(qb.orderByClauses, ", "))
	}

	// SKIP clause
	if qb.skipValue != nil {
		parts = append(parts, fmt.Sprintf("SKIP %d", *qb.skipValue))
	}

	// LIMIT clause
	if qb.limitValue != nil {
		parts = append(parts, fmt.Sprintf("LIMIT %d", *qb.limitValue))
	}

	return strings.Join(parts, " ")
}

// Parameters returns the parameters map for the query.
func (qb *QueryBuilder) Parameters() map[string]interface{} {
	return qb.parameters
}

// NodePattern helps build node patterns for MATCH/CREATE clauses.
type NodePattern struct {
	variable   string
	labels     []string
	properties map[string]interface{}
}

// NewNodePattern creates a new NodePattern builder.
func NewNodePattern(variable string) *NodePattern {
	return &NodePattern{
		variable:   variable,
		labels:     make([]string, 0),
		properties: make(map[string]interface{}),
	}
}

// WithLabel adds a label to the node pattern.
func (np *NodePattern) WithLabel(label string) *NodePattern {
	np.labels = append(np.labels, label)
	return np
}

// WithLabels adds multiple labels to the node pattern.
func (np *NodePattern) WithLabels(labels ...string) *NodePattern {
	np.labels = append(np.labels, labels...)
	return np
}

// WithProperty adds a property to the node pattern.
func (np *NodePattern) WithProperty(key string, value interface{}) *NodePattern {
	np.properties[key] = value
	return np
}

// WithProperties adds multiple properties to the node pattern.
func (np *NodePattern) WithProperties(props map[string]interface{}) *NodePattern {
	for k, v := range props {
		np.properties[k] = v
	}
	return np
}

// Build constructs the node pattern string.
func (np *NodePattern) Build() string {
	var result strings.Builder
	result.WriteString("(")
	result.WriteString(np.variable)

	for _, label := range np.labels {
		result.WriteString(":")
		result.WriteString(label)
	}

	if len(np.properties) > 0 {
		result.WriteString(" {")
		first := true
		for k, v := range np.properties {
			if !first {
				result.WriteString(", ")
			}
			result.WriteString(k)
			result.WriteString(": ")
			result.WriteString(formatValue(v))
			first = false
		}
		result.WriteString("}")
	}

	result.WriteString(")")
	return result.String()
}

// RelationshipPattern helps build relationship patterns.
type RelationshipPattern struct {
	variable   string
	relType    string
	direction  string // "", "->", "<-"
	properties map[string]interface{}
	minHops    *int
	maxHops    *int
}

// NewRelPattern creates a new RelationshipPattern builder.
func NewRelPattern(variable string) *RelationshipPattern {
	return &RelationshipPattern{
		variable:   variable,
		direction:  "->", // default outgoing
		properties: make(map[string]interface{}),
	}
}

// WithType sets the relationship type.
func (rp *RelationshipPattern) WithType(relType string) *RelationshipPattern {
	rp.relType = relType
	return rp
}

// Outgoing sets the relationship direction to outgoing (->).
func (rp *RelationshipPattern) Outgoing() *RelationshipPattern {
	rp.direction = "->"
	return rp
}

// Incoming sets the relationship direction to incoming (<-).
func (rp *RelationshipPattern) Incoming() *RelationshipPattern {
	rp.direction = "<-"
	return rp
}

// Undirected sets the relationship to undirected (-).
func (rp *RelationshipPattern) Undirected() *RelationshipPattern {
	rp.direction = ""
	return rp
}

// WithHops sets variable length path hops.
func (rp *RelationshipPattern) WithHops(min, max int) *RelationshipPattern {
	rp.minHops = &min
	rp.maxHops = &max
	return rp
}

// WithMinHops sets minimum hops for variable length path.
func (rp *RelationshipPattern) WithMinHops(min int) *RelationshipPattern {
	rp.minHops = &min
	return rp
}

// WithMaxHops sets maximum hops for variable length path.
func (rp *RelationshipPattern) WithMaxHops(max int) *RelationshipPattern {
	rp.maxHops = &max
	return rp
}

// Build constructs the relationship pattern string.
func (rp *RelationshipPattern) Build() string {
	var result strings.Builder

	// Start arrow
	if rp.direction == "<-" {
		result.WriteString("<-[")
	} else {
		result.WriteString("-[")
	}

	result.WriteString(rp.variable)

	if rp.relType != "" {
		result.WriteString(":")
		result.WriteString(rp.relType)
	}

	// Variable length
	if rp.minHops != nil || rp.maxHops != nil {
		result.WriteString("*")
		if rp.minHops != nil {
			result.WriteString(fmt.Sprintf("%d", *rp.minHops))
		}
		result.WriteString("..")
		if rp.maxHops != nil {
			result.WriteString(fmt.Sprintf("%d", *rp.maxHops))
		}
	}

	result.WriteString("]-")

	// End arrow
	if rp.direction == "->" {
		result.WriteString(">")
	}

	return result.String()
}

// formatValue formats a value for use in Cypher queries.
func formatValue(v interface{}) string {
	switch val := v.(type) {
	case string:
		return fmt.Sprintf("'%s'", strings.ReplaceAll(val, "'", "\\'"))
	case int, int32, int64, float32, float64:
		return fmt.Sprintf("%v", val)
	case bool:
		if val {
			return "true"
		}
		return "false"
	case nil:
		return "null"
	default:
		return fmt.Sprintf("'%v'", val)
	}
}

// Path helps build path patterns combining nodes and relationships.
func Path(patterns ...string) string {
	return strings.Join(patterns, "")
}
