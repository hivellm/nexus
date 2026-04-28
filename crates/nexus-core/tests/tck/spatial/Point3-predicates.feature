#
# Copyright (c) 2026 Nexus Contributors
# Licensed under the Apache License, Version 2.0
#

#encoding: utf-8

Feature: Point3 - Spatial predicates

  Scenario: [1] point.withinBBox returns true for an interior point
    Given an empty graph
    When executing query:
      """
      RETURN point.withinBBox(point({x: 1.0, y: 1.0}), {bottomLeft: point({x: 0.0, y: 0.0}), topRight: point({x: 2.0, y: 2.0})}) AS v
      """
    Then the result should be, in any order:
      | v    |
      | true |
    And no side effects

  Scenario: [2] point.withinBBox returns false for an exterior point
    Given an empty graph
    When executing query:
      """
      RETURN point.withinBBox(point({x: 3.0, y: 3.0}), {bottomLeft: point({x: 0.0, y: 0.0}), topRight: point({x: 2.0, y: 2.0})}) AS v
      """
    Then the result should be, in any order:
      | v     |
      | false |
    And no side effects

  Scenario: [3] point.withinBBox treats the boundary as inside
    Given an empty graph
    When executing query:
      """
      RETURN point.withinBBox(point({x: 0.0, y: 0.0}), {bottomLeft: point({x: 0.0, y: 0.0}), topRight: point({x: 2.0, y: 2.0})}) AS v
      """
    Then the result should be, in any order:
      | v    |
      | true |
    And no side effects

  Scenario: [4] CRS mismatch on bbox raises ERR_CRS_MISMATCH
    Given an empty graph
    When executing query:
      """
      RETURN point.withinBBox(point({x: 1.0, y: 1.0}), {bottomLeft: point({longitude: 0.0, latitude: 0.0}), topRight: point({longitude: 2.0, latitude: 2.0})}) AS v
      """
    Then a TypeError should be raised at runtime: ERR_CRS_MISMATCH

  Scenario: [5] point.withinDistance — interior point within radius
    Given an empty graph
    When executing query:
      """
      RETURN point.withinDistance(point({x: 1.0, y: 1.0}), point({x: 2.0, y: 1.0}), 5.0) AS v
      """
    Then the result should be, in any order:
      | v    |
      | true |
    And no side effects

  Scenario: [6] point.withinDistance — point outside radius
    Given an empty graph
    When executing query:
      """
      RETURN point.withinDistance(point({x: 0.0, y: 0.0}), point({x: 10.0, y: 0.0}), 5.0) AS v
      """
    Then the result should be, in any order:
      | v     |
      | false |
    And no side effects

  Scenario: [7] point.withinDistance — exact radius is inside
    Given an empty graph
    When executing query:
      """
      RETURN point.withinDistance(point({x: 0.0, y: 0.0}), point({x: 3.0, y: 4.0}), 5.0) AS v
      """
    Then the result should be, in any order:
      | v    |
      | true |
    And no side effects
