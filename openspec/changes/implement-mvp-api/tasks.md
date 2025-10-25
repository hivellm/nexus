# Implementation Tasks - MVP HTTP API

## 1. Cypher Endpoint Implementation

- [ ] 1.1 Connect /cypher endpoint to executor
- [ ] 1.2 Add parameter validation (query, params, timeout)
- [ ] 1.3 Implement query execution with timeout
- [ ] 1.4 Add error handling (syntax errors, execution errors)
- [ ] 1.5 Add response formatting (columns, rows, execution_time)
- [ ] 1.6 Add unit tests (95%+ coverage)

## 2. KNN Traverse Endpoint

- [ ] 2.1 Connect /knn_traverse to KNN index
- [ ] 2.2 Validate vector dimension matches index
- [ ] 2.3 Execute KNN search
- [ ] 2.4 Execute optional graph expansion
- [ ] 2.5 Apply WHERE filters
- [ ] 2.6 Add execution time breakdown (knn, expand, filter)
- [ ] 2.7 Add unit tests (95%+ coverage)

## 3. Ingest Endpoint

- [ ] 3.1 Parse bulk ingestion request
- [ ] 3.2 Batch node creation
- [ ] 3.3 Batch relationship creation
- [ ] 3.4 Handle partial failures (error array)
- [ ] 3.5 Calculate throughput metrics
- [ ] 3.6 Add unit tests (95%+ coverage)

## 4. Streaming Support (SSE)

- [ ] 4.1 Implement Server-Sent Events for large results
- [ ] 4.2 Add chunked transfer encoding
- [ ] 4.3 Add backpressure handling
- [ ] 4.4 Add streaming timeout
- [ ] 4.5 Add unit tests

## 5. Integration & Testing

- [ ] 5.1 API test: POST /cypher with simple query
- [ ] 5.2 API test: POST /cypher with parameters
- [ ] 5.3 API test: POST /knn_traverse
- [ ] 5.4 API test: POST /ingest (bulk load)
- [ ] 5.5 API test: Error handling (400, 408, 500)
- [ ] 5.6 Performance test: API throughput
- [ ] 5.7 Verify 95%+ coverage

## 6. Documentation & Quality

- [ ] 6.1 Update docs/ROADMAP.md (mark Phase 1.5 complete)
- [ ] 6.2 Add API usage examples
- [ ] 6.3 Update CHANGELOG.md with v0.4.0
- [ ] 6.4 Run all quality checks

