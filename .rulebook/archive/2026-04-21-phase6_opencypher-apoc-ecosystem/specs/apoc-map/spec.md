# `apoc.map.*` Procedure Spec

## ADDED Requirements

### Requirement: `apoc.map.merge(a, b)`

The procedure SHALL return a MAP combining `a` and `b`; keys in `b`
SHALL overwrite keys in `a`.

#### Scenario: Overlapping keys
Given `apoc.map.merge({a:1, b:2}, {b:3, c:4})`
Then the result SHALL equal `{a:1, b:3, c:4}`

### Requirement: `apoc.map.fromPairs(pairs)`

The procedure SHALL return a MAP built from a LIST of two-element
lists. Every outer list element MUST be exactly `[key, value]`.

#### Scenario: Typical use
Given `apoc.map.fromPairs([["a", 1], ["b", 2]])`
Then the result SHALL equal `{a: 1, b: 2}`

#### Scenario: Bad pair rejected
Given `apoc.map.fromPairs([["a"]])`
When the procedure is executed
Then the server SHALL respond with HTTP 400
And the error code SHALL be `ERR_INVALID_ARG_VALUE`

### Requirement: `apoc.map.removeKeys(map, keys)`

The procedure SHALL return `map` without the keys listed in `keys`.

#### Scenario: Remove two keys
Given `apoc.map.removeKeys({a:1, b:2, c:3}, ["a", "b"])`
Then the result SHALL equal `{c: 3}`

### Requirement: `apoc.map.clean(map, removeKeys, removeValues)`

The procedure SHALL remove any key in `removeKeys` AND any key whose
value is in `removeValues`.

#### Scenario: Clean NULL and sentinel
Given `apoc.map.clean({a:1, b:null, c:"drop"}, [], [null, "drop"])`
Then the result SHALL equal `{a: 1}`

### Requirement: `apoc.map.flatten(map, delimiter='.')`

The procedure SHALL flatten a nested map into a single-level map
with dotted keys.

#### Scenario: Nested flatten
Given `apoc.map.flatten({a:{b:{c:1}}, d:2})`
Then the result SHALL equal `{"a.b.c": 1, "d": 2}`

### Requirement: `apoc.map.groupBy(list, keyName)`

The procedure SHALL group a LIST of MAPs by the named key, returning
a MAP from key value to the matched MAP (last-wins on collision).

#### Scenario: Typical group
Given `apoc.map.groupBy([{g:"a", v:1}, {g:"b", v:2}, {g:"a", v:3}], "g")`
Then the result SHALL equal `{"a": {g:"a", v:3}, "b": {g:"b", v:2}}`
