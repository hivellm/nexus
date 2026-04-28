#
# Copyright (c) 2026 Nexus Contributors
# Licensed under the Apache License, Version 2.0
#

#encoding: utf-8

Feature: SpatialIndex1 - CREATE SPATIAL INDEX and db.indexes() RTREE rows

  Scenario: [1] CREATE SPATIAL INDEX returns a feedback row
    Given an empty graph
    When executing query:
      """
      CREATE SPATIAL INDEX ON :Store(loc)
      """
    Then the result should be, in any order:
      | index         | message                              |
      | ':Store(loc)' | 'Spatial index :Store(loc) created'  |
    And no side effects

  Scenario: [2] db.indexes() reports the registered RTREE index alongside the auto-LOOKUP entry
    Given an empty graph
    And having executed:
      """
      CREATE SPATIAL INDEX ON :Store(loc)
      """
    When executing query:
      """
      CALL db.indexes()
      """
    Then the result should be, in any order:
      | id | name                | state    | populationPercent | uniqueness   | type     | entityType | labelsOrTypes | properties | indexProvider      | options |
      | 0  | 'index_label_Store' | 'ONLINE' | 100.0             | 'NONUNIQUE'  | 'LOOKUP' | 'NODE'     | ['Store']     | []         | 'token-lookup-1.0' | {}      |
      | 1  | 'Store.loc'         | 'ONLINE' | 100.0             | 'NONUNIQUE'  | 'RTREE'  | 'NODE'     | ['Store']     | ['loc']    | 'rtree-1.0'        | {}      |
    And no side effects

  Scenario: [3] CREATE SPATIAL INDEX rejects a non-Point sample row
    Given an empty graph
    And having executed:
      """
      CREATE (:Store {loc: 'not-a-point', name: 'A'})
      """
    When executing query:
      """
      CREATE SPATIAL INDEX ON :Store(loc)
      """
    Then a ConstraintError should be raised at runtime: ERR_RTREE_BUILD
