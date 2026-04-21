# `apoc.coll.*` Procedure Spec

## ADDED Requirements

### Requirement: `apoc.coll.union(list1, list2)`

The procedure SHALL return a LIST containing every distinct element
of `list1 ∪ list2`, preserving first-seen order.

#### Scenario: Basic union
Given the query `RETURN apoc.coll.union([1, 2, 3], [3, 4, 5])`
When the query is executed
Then the result SHALL equal `[1, 2, 3, 4, 5]`

#### Scenario: NULL input is treated as empty
Given the query `RETURN apoc.coll.union([1, 2], null)`
When the query is executed
Then the result SHALL equal `[1, 2]`

### Requirement: `apoc.coll.intersection(list1, list2)`

The procedure SHALL return elements present in BOTH lists,
preserving order from `list1`.

#### Scenario: Intersection
Given `apoc.coll.intersection([1, 2, 3, 4], [3, 4, 5])`
Then the result SHALL equal `[3, 4]`

### Requirement: `apoc.coll.sort(list)`

The procedure SHALL return `list` sorted ascending using natural
ordering. Mixed-type elements SHALL sort by type ordinal (NULL <
BOOLEAN < INTEGER < FLOAT < STRING < …) matching Neo4j's rule.

#### Scenario: Integer sort
Given `apoc.coll.sort([3, 1, 2])`
Then the result SHALL equal `[1, 2, 3]`

#### Scenario: Mixed-type sort
Given `apoc.coll.sort([1.5, 1, "a", true])`
Then the result SHALL equal `[true, 1, 1.5, "a"]`

### Requirement: `apoc.coll.flatten(list, deep=false)`

The procedure SHALL flatten one level by default, or all levels when
`deep = true`.

#### Scenario: Single-level flatten
Given `apoc.coll.flatten([[1, 2], [3, [4, 5]]])`
Then the result SHALL equal `[1, 2, 3, [4, 5]]`

#### Scenario: Deep flatten
Given `apoc.coll.flatten([[1, 2], [3, [4, 5]]], true)`
Then the result SHALL equal `[1, 2, 3, 4, 5]`

### Requirement: `apoc.coll.frequencies(list)`

The procedure SHALL return a LIST of MAPs `[{item, count}]` with the
count of each distinct element, ordered by count descending.

#### Scenario: Frequency counting
Given `apoc.coll.frequencies(["a", "b", "a", "c", "a", "b"])`
Then the result SHALL equal `[{item: "a", count: 3}, {item: "b", count: 2}, {item: "c", count: 1}]`

### Requirement: `apoc.coll.pairs(list)`

The procedure SHALL return consecutive element pairs including a
trailing `[last, null]` pair.

#### Scenario: Pairs of three
Given `apoc.coll.pairs([1, 2, 3])`
Then the result SHALL equal `[[1, 2], [2, 3], [3, null]]`

### Requirement: NULL Input Never Panics

Every `apoc.coll.*` procedure SHALL accept NULL in any LIST argument
and treat it as an empty list, returning an appropriate scalar or
empty list depending on the procedure's semantics.

#### Scenario: NULL input
Given `apoc.coll.union(null, null)`
Then the result SHALL equal `[]`
