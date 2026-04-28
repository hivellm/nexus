#
# Copyright (c) 2026 Nexus Contributors
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# This Nexus-authored conformance suite mirrors the openCypher TCK
# Gherkin format for the spatial Cypher surface. The upstream
# openCypher TCK has no spatial corpus as of 2026-04-28; see
# tck/spatial/VENDOR.md for the verification details.
#

#encoding: utf-8

Feature: Point1 - Constructing point values

  Scenario: [1] Construct a 2D Cartesian point with positional x/y
    Given an empty graph
    When executing query:
      """
      RETURN point({x: 1.0, y: 2.0}) AS p
      """
    Then the result should be, in any order:
      | p                                              |
      | {x: 1.0, y: 2.0, crs: 'cartesian'}             |
    And no side effects

  Scenario: [2] Construct a 3D Cartesian point with x/y/z
    Given an empty graph
    When executing query:
      """
      RETURN point({x: 1.0, y: 2.0, z: 3.0}) AS p
      """
    Then the result should be, in any order:
      | p                                                          |
      | {x: 1.0, y: 2.0, z: 3.0, crs: 'cartesian-3d'}              |
    And no side effects

  Scenario: [3] Construct a 2D WGS-84 point from longitude/latitude
    Given an empty graph
    When executing query:
      """
      RETURN point({longitude: -73.9857, latitude: 40.7484}) AS p
      """
    Then the result should be, in any order:
      | p                                                          |
      | {x: -73.9857, y: 40.7484, crs: 'wgs-84'}                   |
    And no side effects

  Scenario: [4] Construct a 3D WGS-84 point from longitude/latitude/height
    Given an empty graph
    When executing query:
      """
      RETURN point({longitude: 13.4, latitude: 52.5, height: 100.0}) AS p
      """
    Then the result should be, in any order:
      | p                                                                       |
      | {x: 13.4, y: 52.5, z: 100.0, crs: 'wgs-84-3d'}                          |
    And no side effects

  Scenario: [5] Negative coordinates parse correctly via unary minus
    Given an empty graph
    When executing query:
      """
      RETURN point({x: -1.5, y: -2.5}) AS p
      """
    Then the result should be, in any order:
      | p                                              |
      | {x: -1.5, y: -2.5, crs: 'cartesian'}           |
    And no side effects

  Scenario: [6] Explicit `crs: 'wgs-84'` overrides x/y aliases
    Given an empty graph
    When executing query:
      """
      RETURN point({x: -73.9857, y: 40.7484, crs: 'wgs-84'}) AS p
      """
    Then the result should be, in any order:
      | p                                                          |
      | {x: -73.9857, y: 40.7484, crs: 'wgs-84'}                   |
    And no side effects

  Scenario: [7] Explicit `crs: 'cartesian'` overrides longitude/latitude aliases
    Given an empty graph
    When executing query:
      """
      RETURN point({longitude: 1.0, latitude: 2.0, crs: 'cartesian'}) AS p
      """
    Then the result should be, in any order:
      | p                                              |
      | {x: 1.0, y: 2.0, crs: 'cartesian'}             |
    And no side effects
