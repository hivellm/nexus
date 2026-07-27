//! Mathematical and trigonometric built-in functions for the projection evaluator.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

impl Executor {
    /// Evaluate math / trigonometric built-in functions.
    ///
    /// Returns `None` if the function name is not handled here.
    pub(super) fn eval_builtin_math(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        name: &str,
        args: &[super::super::super::parser::Expression],
    ) -> Option<Result<Value>> {
        match name {
            // Math functions
            "abs" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(num.abs())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "ceil" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(num.ceil())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "floor" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(num.floor())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "round" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(num.round())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "sqrt" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(num.sqrt())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "pow" => {
                // pow(base, exponent)
                if args.len() >= 2 {
                    let base_val = match self.evaluate_projection_expression(row, context, &args[0])
                    {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let exp_val = match self.evaluate_projection_expression(row, context, &args[1])
                    {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if base_val.is_null() || exp_val.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let base = match self.value_to_number(&base_val) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    let exp = match self.value_to_number(&exp_val) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(base.powf(exp))
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "sin" => {
                // sin(angle) - sine function (angle in radians)
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(num.sin())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "cos" => {
                // cos(angle) - cosine function (angle in radians)
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(num.cos())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "tan" => {
                // tan(angle) - tangent function (angle in radians)
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(num.tan())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            // Mathematical functions
            "asin" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(num.asin())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "acos" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(num.acos())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "atan" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(num.atan())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "atan2" => {
                // atan2(y, x) - returns arctangent of y/x
                if args.len() >= 2 {
                    let y_val = match self.evaluate_projection_expression(row, context, &args[0]) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let x_val = match self.evaluate_projection_expression(row, context, &args[1]) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if y_val.is_null() || x_val.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let y = match self.value_to_number(&y_val) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    let x = match self.value_to_number(&x_val) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(y.atan2(x))
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "exp" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(num.exp())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "log" => {
                // phase4_cypher-parity-quick-wins §1.3 — two-arg
                // `log(x, base)` computes the base-`base` logarithm of
                // `x`; the original one-arg form (natural log) is
                // untouched below. NULL in either argument propagates,
                // matching every other arm in this file.
                if args.len() >= 2 {
                    let x_val = match self.evaluate_projection_expression(row, context, &args[0]) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let base_val = match self.evaluate_projection_expression(row, context, &args[1])
                    {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if x_val.is_null() || base_val.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let x = match self.value_to_number(&x_val) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    let base = match self.value_to_number(&base_val) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(x.log(base))
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    // Natural logarithm (ln)
                    return Some(
                        serde_json::Number::from_f64(num.ln())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            // phase4_cypher-parity-quick-wins §1.3 — `isNaN(x)` reports
            // whether `x` is the floating-point NaN value. NULL
            // propagates; non-numeric input goes through the same
            // `value_to_number` conversion (and TypeMismatch error) as
            // every other math builtin above, rather than silently
            // returning false.
            "isnan" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(Ok(Value::Bool(num.is_nan())));
                }
                Some(Ok(Value::Null))
            }
            "log10" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(num.log10())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "radians" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    // Convert degrees to radians
                    return Some(
                        serde_json::Number::from_f64(num.to_radians())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "degrees" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    // Convert radians to degrees
                    return Some(
                        serde_json::Number::from_f64(num.to_degrees())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            "pi" => {
                // pi() - returns the mathematical constant π
                Some(Ok(Value::Number(
                    serde_json::Number::from_f64(std::f64::consts::PI).unwrap(),
                )))
            }
            "e" => {
                // e() - returns the mathematical constant e
                Some(Ok(Value::Number(
                    serde_json::Number::from_f64(std::f64::consts::E).unwrap(),
                )))
            }
            // phase21_tck-missing-functions — `sign(x)` returns -1, 0, or 1
            // depending on the sign of `x` (INTEGER result). NULL propagates;
            // invalid types raise the same `TypeMismatch` every other math
            // arm above raises via `value_to_number`. `f64::signum` is NOT
            // used directly because it maps `0.0.signum() == 1.0`, which
            // would wrongly report `sign(0) == 1` instead of `0`.
            "sign" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    let sign: i64 = if num > 0.0 {
                        1
                    } else if num < 0.0 {
                        -1
                    } else {
                        0
                    };
                    return Some(Ok(Value::Number(sign.into())));
                }
                Some(Ok(Value::Null))
            }
            // phase21_tck-missing-functions — `rand()` returns a FLOAT in
            // `[0, 1)` from the thread-local RNG. No arguments, so no NULL
            // to propagate (same shape as `randomuuid()` in fn_graph.rs).
            "rand" => {
                let value = rand::random::<f64>();
                Some(
                    serde_json::Number::from_f64(value)
                        .map(Value::Number)
                        .ok_or_else(|| Error::TypeMismatch {
                            expected: "number".to_string(),
                            actual: "non-finite".to_string(),
                        }),
                )
            }
            // phase21_tck-missing-functions — `cot(x)` is the cotangent,
            // `1 / tan(x)`. NULL/type handling mirrors `tan` above.
            "cot" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64(1.0 / num.tan())
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            // phase21_tck-missing-functions — `haversin(x)` is the
            // haversine function, `(1 - cos(x)) / 2`, used in great-circle
            // distance formulas. NULL/type handling mirrors `cos` above.
            "haversin" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if value.is_null() {
                        return Some(Ok(Value::Null));
                    }
                    let num = match self.value_to_number(&value) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(
                        serde_json::Number::from_f64((1.0 - num.cos()) / 2.0)
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            _ => None,
        }
    }
}
