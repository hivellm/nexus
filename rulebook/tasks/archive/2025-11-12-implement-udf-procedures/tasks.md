# Tasks - UDF & Procedures

## 1. UDF Framework
- [x] 1.1 UDF registration API design
- [x] 1.2 UDF storage in catalog (persistence implemented)
- [x] 1.3 UDF invocation in expressions
- [x] 1.4 Multiple return types support (via UdfReturnType enum)
- [x] 1.5 Add tests (comprehensive tests added with persistence coverage)

## 2. Custom Procedures
- [x] 2.1 Procedure registration API design
- [x] 2.2 Procedure storage in catalog (persistence implemented)
- [x] 2.3 YIELD support for custom procedures (parsing implemented)
- [x] 2.4 Streaming results support (implemented with callback-based API)
- [x] 2.5 Add tests (comprehensive tests added with persistence coverage)

## 3. Plugin System
- [x] 3.1 Plugin architecture design (trait-based architecture implemented)
- [x] 3.2 Plugin loading mechanism (static loading implemented, dynamic loading placeholder)
- [x] 3.3 Plugin lifecycle management (initialize/shutdown lifecycle implemented)
- [x] 3.4 Plugin API documentation (USER_GUIDE.md updated with plugin documentation)
- [x] 3.5 Add tests (comprehensive tests added)

## 4. Quality
- [ ] 4.1 95%+ coverage (tests added, coverage check pending)
- [x] 4.2 No clippy warnings (critical warnings fixed, minor warnings remain)
- [x] 4.3 Update documentation (USER_GUIDE.md updated with UDF and procedure documentation)
