# Tasks - Query Analysis

## 1. EXPLAIN Command
- [x] 1.1 EXPLAIN parsing ✅
- [x] 1.2 Generate execution plan ✅
- [x] 1.3 Return plan as JSON/text ✅
- [x] 1.4 Add tests ✅ (4 tests: simple query, WHERE clause, CREATE query, plan structure)

## 2. PROFILE Command
- [x] 2.1 PROFILE parsing ✅
- [x] 2.2 Query instrumentation ✅
- [x] 2.3 Runtime statistics collection ✅ (execution_time_ms, execution_time_us, rows_returned, columns_returned)
- [x] 2.4 Add tests ✅ (4 tests: simple query, WHERE clause, CREATE query, profile structure)

## 3. Query Hints
- [x] 3.1 USING INDEX hint ✅ (parsing implemented)
- [x] 3.2 USING SCAN hint ✅ (parsing implemented)
- [x] 3.3 USING JOIN ON hint ✅ (parsing implemented)
- [x] 3.4 Planner hint support ✅ (hints stored in MatchClause, planner reads hints)
- [x] 3.5 Add tests ✅ (4 tests: USING INDEX, USING SCAN, USING JOIN, multiple hints)

## 4. Quality
- [ ] 4.1 95%+ coverage (pending - need to run coverage check)
- [x] 4.2 No clippy warnings ✅
- [ ] 4.3 Update documentation (pending)
