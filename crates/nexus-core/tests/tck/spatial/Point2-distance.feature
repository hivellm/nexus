#
# Copyright (c) 2026 Nexus Contributors
# Licensed under the Apache License, Version 2.0
#

#encoding: utf-8

Feature: Point2 - Distance between points

  Scenario: [1] Cartesian 2D distance — Pythagorean triple
    Given an empty graph
    When executing query:
      """
      RETURN point.distance(point({x: 0.0, y: 0.0}), point({x: 3.0, y: 4.0})) AS d
      """
    Then the result should be, in any order:
      | d   |
      | 5.0 |
    And no side effects

  Scenario: [2] Distance is symmetric
    Given an empty graph
    When executing query:
      """
      RETURN point.distance(point({x: 1.0, y: 1.0}), point({x: 4.0, y: 5.0})) AS a, point.distance(point({x: 4.0, y: 5.0}), point({x: 1.0, y: 1.0})) AS b
      """
    Then the result should be, in any order:
      | a   | b   |
      | 5.0 | 5.0 |
    And no side effects

  Scenario: [3] Distance from a point to itself is zero
    Given an empty graph
    When executing query:
      """
      RETURN point.distance(point({x: 7.0, y: -3.0}), point({x: 7.0, y: -3.0})) AS d
      """
    Then the result should be, in any order:
      | d   |
      | 0.0 |
    And no side effects

  Scenario: [4] Cartesian 3D distance — Pythagorean across all axes
    Given an empty graph
    When executing query:
      """
      RETURN point.distance(point({x: 0.0, y: 0.0, z: 0.0}), point({x: 1.0, y: 2.0, z: 2.0})) AS d
      """
    Then the result should be, in any order:
      | d   |
      | 3.0 |
    And no side effects

  Scenario: [5] CRS mismatch raises ERR_CRS_MISMATCH
    Given an empty graph
    When executing query:
      """
      RETURN point.distance(point({x: 1.0, y: 2.0}), point({longitude: 1.0, latitude: 2.0})) AS d
      """
    Then a TypeError should be raised at runtime: ERR_CRS_MISMATCH
