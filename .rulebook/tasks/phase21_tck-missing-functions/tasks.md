## 1. Implementation
- [x] 1.1 reverse(string) added (fn_list.rs — reverses characters; array reverse unchanged; NULL propagates)
- [x] 1.2 range(step=0) now errors ("step argument to range() cannot be zero") instead of returning an empty list (fn_list.rs)
- [x] 1.3 sign / rand / cot / haversin added (fn_math.rs; rand via rand::random::<f64>())
- [x] 1.4 properties(node|rel|map) added (fn_graph.rs — strips _nexus_* markers; for a rel also strips the `type` alias; NULL propagates)
- [ ] 1.5 startNode(rel) / endNode(rel) — DEFERRED (genuine dependency): the relationship VALUE carries no endpoint ids (only _nexus_id/_nexus_rel_type/type + props), so resolving endpoints needs graph/store access in the projection evaluator, which it does not currently have. Needs a graph-access-aware evaluation path — larger than a pure function add. Follow-up.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation (code comments per function; startNode/endNode deferral documented here)
- [x] 2.2 Write tests covering the new behavior (new_functions_test.rs: reverse_string, range_with_zero_step_errors, sign, cot_and_haversin, rand_is_in_unit_interval, properties_of_map_and_node)
- [x] 2.3 Run tests and confirm they pass (cypher new_functions_test 17/0; clippy/fmt clean)
