## 1. Implementation
- [ ] 1.1 Implement point property accessors in the expression evaluator: cartesian x/y/z + crs ("cartesian"/"cartesian-3d"), WGS-84 longitude/latitude/height + crs ("wgs-84"/"wgs-84-3d")
- [ ] 1.2 Support 3D point construction: point({x, y, z}) and point({longitude, latitude, height})
- [ ] 1.3 Accept the Neo4j positional argument form for point.withinBBox(point, bottomLeft, topRight)

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior (the 10 section-18 compatibility-suite queries as unit tests)
- [ ] 2.3 Run tests and confirm they pass (compatibility suite 325/325)
