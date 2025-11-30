#!/bin/bash

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Source the main test script functions
source scripts/test-neo4j-nexus-compatibility-200.sh

# Initialize counters
PASSED_TESTS=0
FAILED_TESTS=0
SKIPPED_TESTS=0

echo "=== Testing Section 7 and 10 Only ==="

#= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = 
# SECTION 7: RELATIONSHIP QUERIES (30 tests)
#= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = 
clear_databases "Section 7: Relationship Queries"
setup_test_data "relationships"
print_color "$YELLOW" ""
print_color "$YELLOW" "+-----------------------------------------------------+ "
print_color "$YELLOW" "| Section 7: Relationship Queries (30 tests)         |"
print_color "$YELLOW" "+-----------------------------------------------------+ "

run_test "7.01 Match relationship with type" "MATCH (n)-[r:WORKS_AT]->() RETURN count(r) AS cnt"
run_test "7.02 Match specific relationship" "MATCH (n:Person)-[r:WORKS_AT]->(c:Company) RETURN n.name AS person, c.name AS company"
run_test "7.03 Count relationships" "MATCH ()-[r:WORKS_AT]->() RETURN count(r) AS total"

#= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = 
# SECTION 10: UNION QUERIES (10 tests)
#= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = 
clear_databases "Section 10: UNION Queries"
# Setup test data for UNION tests
setup_test_data "basic"
print_color "$YELLOW" ""
print_color "$YELLOW" "+-----------------------------------------------------+ "
print_color "$YELLOW" "| Section 10: UNION Queries (10 tests)               |"
print_color "$YELLOW" "+-----------------------------------------------------+ "

run_test "10.01 UNION two queries" "MATCH (n:Person) RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name"
run_test "10.02 UNION ALL" "MATCH (n:Person) RETURN n.name AS name UNION ALL MATCH (n:Company) RETURN n.name AS name"
run_test "10.05 UNION with WHERE" "MATCH (n:Person) WHERE n.age > 30 RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name"
run_test "10.06 UNION with COUNT" "MATCH (n:Person) RETURN count(n) AS cnt UNION MATCH (n:Company) RETURN count(n) AS cnt"

#= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = 
# FINAL REPORT
#= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = 
print_color "$CYAN" ""
print_color "$CYAN" "+= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = +"
print_color "$CYAN" "|                     TEST SUMMARY                            |"
print_color "$CYAN" "+= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = +"
echo ""

TOTAL_TESTS=$((PASSED_TESTS + FAILED_TESTS + SKIPPED_TESTS))
echo -n "Total Tests:   "
print_color "$WHITE" "$TOTAL_TESTS"
echo -n "Passed:        "
print_color "$GREEN" "$PASSED_TESTS"
echo -n "Failed:        "
print_color "$RED" "$FAILED_TESTS"

