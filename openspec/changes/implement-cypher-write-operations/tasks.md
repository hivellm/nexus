# Implementation Tasks - Cypher Write Operations

**Status**: ⚠️ PARTIALLY STARTED (v0.9.1)

**Recent Implementation (2025-10-27)**:
- ✅ CREATE clause parsing and execution (v0.9.0)
- ✅ MergeClause added to parser AST (v0.9.0)
- ✅ Engine integration for node persistence (v0.9.0)
- ✅ MATCH queries now return results (v0.9.1)
- ✅ Engine-Executor data synchronization (v0.9.1)
- ✅ label_index auto-update in create_node (v0.9.1)
- ✅ MERGE clause execution (v0.9.1 - simplified, creates without checking existing)
- ⏳ MERGE match-or-create semantics pending (Task 1.2)
- ⏳ SET, DELETE, REMOVE clauses pending

## 0. CREATE Clause (COMPLETED ✅)

- [x] 0.1 CREATE clause parsing in parser.rs
- [x] 0.2 CREATE execution via Engine integration
- [x] 0.3 Property parsing with proper whitespace handling
- [x] 0.4 Node creation with labels and properties
- [x] 0.5 Parser fixes for clause recognition
- [x] 0.6 Integration tests for CREATE operations

**Implemented in**:
- Commit: 51bbb32 (parser fixes)
- Commit: 417be25 (CREATE persistence)
- Commit: ede99eb (data unification v0.9.1)
- Files: `nexus-core/src/executor/parser.rs`, `nexus-core/src/lib.rs`, `nexus-server/src/api/cypher.rs`, `nexus-server/src/api/data.rs`

## 1. MERGE Clause

- [x] 1.1 Add MergeClause to parser AST ✅ (v0.9.0)
- [ ] 1.2 Implement match-or-create semantics ⏳ (simplified version done, TODO: add property matching)
- [ ] 1.3 Add ON CREATE/ON MATCH support
- [ ] 1.4 Add MERGE tests ⚠️ (basic execution works, need dedicated MERGE tests)

**Implemented in**:
- Commit: v0.9.1 (MERGE execution)
- Files: `nexus-server/src/api/cypher.rs` (lines 182-244)
- Note: Currently MERGE just creates nodes without checking if they exist. Full match-or-create semantics (Task 1.2) still pending.

## 2. SET Clause
- [ ] 2.1 Add SetClause to parser
- [ ] 2.2 Implement property updates
- [ ] 2.3 Implement label addition
- [ ] 2.4 Add SET tests

## 3. DELETE Clause
- [ ] 3.1 Add DeleteClause to parser
- [ ] 3.2 Implement node/relationship deletion
- [ ] 3.3 Implement DETACH DELETE
- [ ] 3.4 Add DELETE tests

## 4. REMOVE Clause
- [ ] 4.1 Add RemoveClause to parser
- [ ] 4.2 Implement property/label removal
- [ ] 4.3 Add REMOVE tests

## 5. Quality
- [ ] 5.1 95%+ coverage
- [ ] 5.2 No clippy warnings
- [ ] 5.3 Update documentation

---

**Progress**: 7/23 tasks (30% complete)  
**Next Priority**: 
1. Implement MERGE match-or-create semantics (Task 1.2)
2. Add MERGE tests (Task 1.4)
3. Then proceed to SET clause implementation
