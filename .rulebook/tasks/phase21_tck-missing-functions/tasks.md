## 1. Implementation
- [x] 1.1 reverse(string) added (fn_list.rs — reverses characters; array reverse unchanged; NULL propagates)
- [x] 1.2 range(step=0) now errors ("step argument to range() cannot be zero") instead of returning an empty list (fn_list.rs)
- [x] 1.3 sign / rand / cot / haversin added (fn_math.rs; rand via rand::random::<f64>())
- [x] 1.4 properties(node|rel|map) added (fn_graph.rs — strips _nexus_* markers; for a rel also strips the `type` alias; NULL propagates)
- [x] 1.5 startNode(rel) / endNode(rel) added (fn_graph.rs). The prior deferral premise was refuted: the projection evaluator DOES have store access (`self.store()`, already used by `type`/`labels`/`__label_predicate__`). Resolution mirrors `type(rel)` exactly — read the rel's `_nexus_id`, `store().read_rel(rid)` → `src_id`/`dst_id`, then `read_node_as_value(id)` materialises the endpoint as a full node value. Guards with `is_relationship_value` so a node id is never mis-read as a rel id; NULL propagates; non-relationship yields NULL.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation (code comments per function; startNode/endNode resolution documented in fn_graph.rs)
- [x] 2.2 Write tests covering the new behavior (new_functions_test.rs: reverse_string, range_with_zero_step_errors, sign, cot_and_haversin, rand_is_in_unit_interval, properties_of_map_and_node, start_node_and_end_node_return_endpoints, start_node_end_node_null_propagates)
- [x] 2.3 Run tests and confirm they pass (cypher new_functions_test 19/0; clippy/fmt clean)
