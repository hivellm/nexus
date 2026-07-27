# Preserve integer arithmetic: both_as_i64 + checked_* fallback

**Category**: code
**Tags**: cypher, arithmetic, type-preservation, json-value

## Description

When implementing Cypher-style arithmetic on JSON-backed values, check "both operands are pure integers" first and run the i64 path with overflow-checked arithmetic; only fall through to f64 when either operand is a float or the i64 op would overflow. This matches openCypher's "integer stays integer until a float operand is introduced" rule and avoids spurious `7.0` results for `1 + 2 * 3`.

## Example

fn both_as_i64(left: &Value, right: &Value) -> Option<(i64, i64)> {
    let l = left.as_i64()?;
    let r = right.as_i64()?;
    if left.as_f64()?.fract() != 0.0 || right.as_f64()?.fract() != 0.0 { return None; }
    if matches!(left, Value::Number(n) if n.is_f64())
        || matches!(right, Value::Number(n) if n.is_f64()) { return None; }
    Some((l, r))
}

// ...inside add_values:
if let Some((li, ri)) = both_as_i64(left, right) {
    if let Some(sum) = li.checked_add(ri) {
        return Ok(Value::Number(serde_json::Number::from(sum)));
    }
    // overflow — fall through to f64 path
}
// ...f64 path unchanged

## When to Use

Any numeric op evaluator that defaults to f64 but needs to respect the language's integer-preservation contract.

## When NOT to Use

Ops that are intrinsically float (power `^`, trig, log, sqrt) — those should always return float even for integer inputs, matching Cypher semantics.
