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
- [x] 1.2 Implement match-or-create semantics ✅ (v0.9.1 with property matching)
- [ ] 1.3 Add ON CREATE/ON MATCH support
- [x] 1.4 Add MERGE tests ✅ (basic execution works, dedicated MERGE tests added)

**Implemented in**:
- Commit: v0.9.1 (MERGE execution)
- Files: `nexus-server/src/api/cypher.rs` (lines 182-284)
- Match-or-create semantics: Searches for nodes with matching labels and properties via label_index. Creates new node only if no match is found.

## 2. SET Clause
- [x] 2.1 Add SetClause to parser ✅ (v0.9.1)
- [ ] 2.2 Implement property updates ⏳ (requires execution context with variables from MATCH)
- [ ] 2.3 Implement label addition ⏳ (requires execution context with variables from MATCH)
- [ ] 2.4 Add SET tests

**Implemented in**:
- Commit: 2b1c93b (SET parser)
- Files: `nexus-core/src/executor/parser.rs`
- Note: Parser supports SET clause with property updates and label additions. Execution logic pending (requires binding to variables from MATCH clause).

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

**Progress**: 10/23 tasks (43% complete)  
**Next Priority**: 
1. ✅ MERGE match-or-create semantics completed (Task 1.2, 1.4)
2. ✅ SET clause parser completed (Task 2.1)
3. Implement SET clause execution for property updates (Task 2.2, 2.3) - requires execution context
4. Then proceed to DELETE clause
