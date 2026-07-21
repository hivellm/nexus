# openCypher null-positioning for ORDER BY uses a sort-specific comparator

**Category**: code
**Tags**: cypher, order-by, null-semantics, sorting

## Description

openCypher ORDER BY puts NULLs last in ASC and first in DESC — the opposite polarity of what most base comparators return (where null-less-than-everything is correct for WHERE / `<` / `>` predicate semantics). Do NOT change the base comparator to satisfy the sort rule, because predicate comparisons still need the internal total order with null-as-less. Instead, wrap a sort-specific null-aware comparator around the base one, returning the FINAL ordering (both the ASC/DESC reversal AND the null positioning folded in), and drop the caller's `ordering.reverse()` for DESC. Two code paths usually need the wrapper: the standard sort_by and the top-K sort optimisation.

## Example

fn cypher_null_aware_order<F>(left: &Value, right: &Value, ascending: bool, base_cmp: F) -> Ordering
where F: FnOnce(&Value, &Value) -> Ordering,
{
    match (left.is_null(), right.is_null()) {
        (true, true) => Ordering::Equal,
        (true, false) => if ascending { Ordering::Greater } else { Ordering::Less },
        (false, true) => if ascending { Ordering::Less } else { Ordering::Greater },
        (false, false) => {
            let base = base_cmp(left, right);
            if ascending { base } else { base.reverse() }
        }
    }
}

## When to Use

Sorting a result set where `ORDER BY col [ASC|DESC]` needs to follow openCypher / SQL-null-positioning rules, and the base value comparator is also used by predicate evaluation.

## When NOT to Use

Comparisons inside WHERE / boolean expressions — those should keep the base total order (null < non-null) so `x < 5` on a null `x` doesn't accidentally become true.
