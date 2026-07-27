//! String, regex, and bytes built-in functions for the projection evaluator.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Hard cap on the `length` argument accepted by `lpad`/`rpad`. Bounds
/// the resulting `String` to a few MB at worst (each `char` is at most
/// 4 UTF-8 bytes: `1_000_000 * 4 = 4 MB`), well clear of any reasonable
/// per-value budget, while comfortably covering legitimate padding use
/// (report columns, fixed-width IDs, etc).
const MAX_PAD_LEN: usize = 1_000_000;

impl Executor {
    /// Evaluate string, regex, and bytes built-in functions.
    ///
    /// Returns `None` if the function name is not handled here so the caller
    /// can fall through to the next group.
    pub(super) fn eval_builtin_string(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        name: &str,
        args: &[super::super::super::parser::Expression],
    ) -> Option<Result<Value>> {
        match name {
            // String functions
            "tolower" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::String(s) = value {
                        return Some(Ok(Value::String(s.to_lowercase())));
                    }
                }
                Some(Ok(Value::Null))
            }
            "toupper" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::String(s) = value {
                        return Some(Ok(Value::String(s.to_uppercase())));
                    }
                }
                Some(Ok(Value::Null))
            }
            "substring" => {
                // substring(string, start, [length])
                if args.len() >= 2 {
                    let string_val =
                        match self.evaluate_projection_expression(row, context, &args[0]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let start_val =
                        match self.evaluate_projection_expression(row, context, &args[1]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };

                    if let (Value::String(s), Value::Number(start_num)) = (string_val, start_val) {
                        let char_len = s.chars().count() as i64;
                        // Handle both integer and float numbers (floats come from unary minus)
                        let start_i64 = start_num
                            .as_i64()
                            .or_else(|| start_num.as_f64().map(|f| f as i64))
                            .unwrap_or(0);

                        // Handle negative indices (count from end)
                        let start = if start_i64 < 0 {
                            ((char_len + start_i64).max(0)) as usize
                        } else {
                            start_i64.min(char_len) as usize
                        };

                        if args.len() >= 3 {
                            let length_val =
                                match self.evaluate_projection_expression(row, context, &args[2]) {
                                    Ok(v) => v,
                                    Err(e) => return Some(Err(e)),
                                };
                            if let Value::Number(len_num) = length_val {
                                let length = len_num.as_i64().unwrap_or(0).max(0) as usize;
                                let chars: Vec<char> = s.chars().collect();
                                let end = (start + length).min(chars.len());
                                return Some(Ok(Value::String(chars[start..end].iter().collect())));
                            }
                        } else {
                            // No length specified - take from start to end
                            let chars: Vec<char> = s.chars().collect();
                            return Some(Ok(Value::String(chars[start..].iter().collect())));
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "trim" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::String(s) = value {
                        return Some(Ok(Value::String(s.trim().to_string())));
                    }
                }
                Some(Ok(Value::Null))
            }
            "ltrim" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::String(s) = value {
                        return Some(Ok(Value::String(s.trim_start().to_string())));
                    }
                }
                Some(Ok(Value::Null))
            }
            "rtrim" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::String(s) = value {
                        return Some(Ok(Value::String(s.trim_end().to_string())));
                    }
                }
                Some(Ok(Value::Null))
            }
            "replace" => {
                // replace(string, search, replace)
                if args.len() >= 3 {
                    let string_val =
                        match self.evaluate_projection_expression(row, context, &args[0]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let search_val =
                        match self.evaluate_projection_expression(row, context, &args[1]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let replace_val =
                        match self.evaluate_projection_expression(row, context, &args[2]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };

                    if let (Value::String(s), Value::String(search), Value::String(replace)) =
                        (string_val, search_val, replace_val)
                    {
                        return Some(Ok(Value::String(s.replace(&search, &replace))));
                    }
                }
                Some(Ok(Value::Null))
            }
            "split" => {
                // split(string, delimiter)
                if args.len() >= 2 {
                    let string_val =
                        match self.evaluate_projection_expression(row, context, &args[0]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let delim_val =
                        match self.evaluate_projection_expression(row, context, &args[1]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };

                    if let (Value::String(s), Value::String(delim)) = (string_val, delim_val) {
                        let parts: Vec<Value> = s
                            .split(&delim)
                            .map(|part| Value::String(part.to_string()))
                            .collect();
                        return Some(Ok(Value::Array(parts)));
                    }
                }
                Some(Ok(Value::Null))
            }
            // Regex functions
            "regexmatch" => {
                // regexMatch(string, pattern) - returns true if pattern matches string
                if args.len() >= 2 {
                    let string_val =
                        match self.evaluate_projection_expression(row, context, &args[0]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let pattern_val =
                        match self.evaluate_projection_expression(row, context, &args[1]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };

                    if let (Value::String(s), Value::String(pattern)) = (string_val, pattern_val) {
                        match regex::Regex::new(&pattern) {
                            Ok(re) => return Some(Ok(Value::Bool(re.is_match(&s)))),
                            Err(_) => return Some(Ok(Value::Bool(false))),
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "regexreplace" => {
                // regexReplace(string, pattern, replacement) - replaces first match
                if args.len() >= 3 {
                    let string_val =
                        match self.evaluate_projection_expression(row, context, &args[0]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let pattern_val =
                        match self.evaluate_projection_expression(row, context, &args[1]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let replacement_val =
                        match self.evaluate_projection_expression(row, context, &args[2]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };

                    if let (Value::String(s), Value::String(pattern), Value::String(replacement)) =
                        (string_val, pattern_val, replacement_val)
                    {
                        match regex::Regex::new(&pattern) {
                            Ok(re) => {
                                // Replace only the first match
                                let result = re.replace(&s, replacement.as_str());
                                return Some(Ok(Value::String(result.into_owned())));
                            }
                            Err(_) => return Some(Ok(Value::String(s))),
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "regexreplaceall" => {
                // regexReplaceAll(string, pattern, replacement) - replaces all matches
                if args.len() >= 3 {
                    let string_val =
                        match self.evaluate_projection_expression(row, context, &args[0]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let pattern_val =
                        match self.evaluate_projection_expression(row, context, &args[1]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let replacement_val =
                        match self.evaluate_projection_expression(row, context, &args[2]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };

                    if let (Value::String(s), Value::String(pattern), Value::String(replacement)) =
                        (string_val, pattern_val, replacement_val)
                    {
                        match regex::Regex::new(&pattern) {
                            Ok(re) => {
                                // Replace all matches
                                let result = re.replace_all(&s, replacement.as_str());
                                return Some(Ok(Value::String(result.into_owned())));
                            }
                            Err(_) => return Some(Ok(Value::String(s))),
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "regexextract" => {
                // regexExtract(string, pattern) - extracts first match
                if args.len() >= 2 {
                    let string_val =
                        match self.evaluate_projection_expression(row, context, &args[0]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let pattern_val =
                        match self.evaluate_projection_expression(row, context, &args[1]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };

                    if let (Value::String(s), Value::String(pattern)) = (string_val, pattern_val) {
                        match regex::Regex::new(&pattern) {
                            Ok(re) => {
                                if let Some(m) = re.find(&s) {
                                    return Some(Ok(Value::String(m.as_str().to_string())));
                                }
                                return Some(Ok(Value::Null));
                            }
                            Err(_) => return Some(Ok(Value::Null)),
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "regexextractall" => {
                // regexExtractAll(string, pattern) - extracts all matches as array
                if args.len() >= 2 {
                    let string_val =
                        match self.evaluate_projection_expression(row, context, &args[0]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let pattern_val =
                        match self.evaluate_projection_expression(row, context, &args[1]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };

                    if let (Value::String(s), Value::String(pattern)) = (string_val, pattern_val) {
                        match regex::Regex::new(&pattern) {
                            Ok(re) => {
                                let matches: Vec<Value> = re
                                    .find_iter(&s)
                                    .map(|m| Value::String(m.as_str().to_string()))
                                    .collect();
                                return Some(Ok(Value::Array(matches)));
                            }
                            Err(_) => return Some(Ok(Value::Array(vec![]))),
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "regexextractgroups" => {
                // regexExtractGroups(string, pattern) - extracts capture groups from first match
                if args.len() >= 2 {
                    let string_val =
                        match self.evaluate_projection_expression(row, context, &args[0]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let pattern_val =
                        match self.evaluate_projection_expression(row, context, &args[1]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };

                    if let (Value::String(s), Value::String(pattern)) = (string_val, pattern_val) {
                        match regex::Regex::new(&pattern) {
                            Ok(re) => {
                                if let Some(caps) = re.captures(&s) {
                                    let groups: Vec<Value> = caps
                                        .iter()
                                        .skip(1) // Skip the full match (group 0)
                                        .map(|m| {
                                            m.map(|m| Value::String(m.as_str().to_string()))
                                                .unwrap_or(Value::Null)
                                        })
                                        .collect();
                                    return Some(Ok(Value::Array(groups)));
                                }
                                return Some(Ok(Value::Null));
                            }
                            Err(_) => return Some(Ok(Value::Null)),
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "regexsplit" => {
                // regexSplit(string, pattern) - splits string by regex pattern
                if args.len() >= 2 {
                    let string_val =
                        match self.evaluate_projection_expression(row, context, &args[0]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let pattern_val =
                        match self.evaluate_projection_expression(row, context, &args[1]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };

                    if let (Value::String(s), Value::String(pattern)) = (string_val, pattern_val) {
                        match regex::Regex::new(&pattern) {
                            Ok(re) => {
                                let parts: Vec<Value> = re
                                    .split(&s)
                                    .map(|part| Value::String(part.to_string()))
                                    .collect();
                                return Some(Ok(Value::Array(parts)));
                            }
                            Err(_) => {
                                // Fallback to returning original string in array
                                return Some(Ok(Value::Array(vec![Value::String(s)])));
                            }
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            // phase6_opencypher-quickwins §4 — left / right UTF-8-safe
            // prefix / suffix extraction.
            "left" => {
                if args.len() < 2 {
                    return Some(Ok(Value::Null));
                }
                let s_val = match self.evaluate_projection_expression(row, context, &args[0]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let n_val = match self.evaluate_projection_expression(row, context, &args[1]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                if matches!(s_val, Value::Null) || matches!(n_val, Value::Null) {
                    return Some(Ok(Value::Null));
                }
                let s = match s_val {
                    Value::String(s) => s,
                    _ => return Some(Ok(Value::Null)),
                };
                let n = match n_val {
                    Value::Number(n) => n
                        .as_i64()
                        .or_else(|| n.as_f64().map(|f| f as i64))
                        .unwrap_or(0),
                    _ => return Some(Ok(Value::Null)),
                };
                let take = n.max(0) as usize;
                Some(Ok(Value::String(s.chars().take(take).collect())))
            }
            "right" => {
                if args.len() < 2 {
                    return Some(Ok(Value::Null));
                }
                let s_val = match self.evaluate_projection_expression(row, context, &args[0]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let n_val = match self.evaluate_projection_expression(row, context, &args[1]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                if matches!(s_val, Value::Null) || matches!(n_val, Value::Null) {
                    return Some(Ok(Value::Null));
                }
                let s = match s_val {
                    Value::String(s) => s,
                    _ => return Some(Ok(Value::Null)),
                };
                let n = match n_val {
                    Value::Number(n) => n
                        .as_i64()
                        .or_else(|| n.as_f64().map(|f| f as i64))
                        .unwrap_or(0),
                    _ => return Some(Ok(Value::Null)),
                };
                let char_len = s.chars().count();
                let take = (n.max(0) as usize).min(char_len);
                let skip = char_len - take;
                Some(Ok(Value::String(s.chars().skip(skip).collect())))
            }
            // phase4_cypher-parity-quick-wins §1.2 — `ascii(s)` returns the
            // Unicode code point of the first character of `s` (matching
            // openCypher/Neo4j semantics: character, not byte). Empty
            // string and non-STRING input return NULL, mirroring the
            // `left`/`right` type-error convention above.
            "ascii" => {
                let arg = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                match arg {
                    Value::String(s) => match s.chars().next() {
                        Some(c) => Some(Ok(Value::Number((c as u32 as i64).into()))),
                        None => Some(Ok(Value::Null)),
                    },
                    _ => Some(Ok(Value::Null)),
                }
            }
            // phase4_cypher-parity-quick-wins §1.2 — `chr(n)` is the
            // inverse of `ascii()`: builds a one-character string from a
            // Unicode code point. Code points that are not valid Unicode
            // scalar values (e.g. UTF-16 surrogate halves) return NULL
            // rather than erroring, since `char::from_u32` already gives
            // us that distinction for free.
            "chr" => {
                let arg = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                let code = match arg {
                    Value::Number(n) => n
                        .as_i64()
                        .or_else(|| n.as_f64().map(|f| f as i64))
                        .and_then(|i| u32::try_from(i).ok()),
                    _ => None,
                };
                match code.and_then(char::from_u32) {
                    Some(c) => Some(Ok(Value::String(c.to_string()))),
                    None => Some(Ok(Value::Null)),
                }
            }
            // phase4_cypher-parity-quick-wins §1.2 — `lpad`/`rpad(original,
            // length, [padString])`. `padString` defaults to a single
            // space when omitted. When `original` is already at least
            // `length` characters long, both truncate to the first
            // `length` characters (Oracle-style LPAD/RPAD semantics: the
            // truncation is a plain prefix-substring, independent of the
            // padding side).
            "lpad" | "rpad" => {
                if args.len() < 2 {
                    return Some(Ok(Value::Null));
                }
                let s_val = match self.evaluate_projection_expression(row, context, &args[0]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let len_val = match self.evaluate_projection_expression(row, context, &args[1]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let pad_val = match args.get(2) {
                    Some(expr) => match self.evaluate_projection_expression(row, context, expr) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => Value::String(" ".to_string()),
                };
                if matches!(s_val, Value::Null)
                    || matches!(len_val, Value::Null)
                    || matches!(pad_val, Value::Null)
                {
                    return Some(Ok(Value::Null));
                }
                let s = match s_val {
                    Value::String(s) => s,
                    _ => return Some(Ok(Value::Null)),
                };
                let target_len = match &len_val {
                    Value::Number(n) => n
                        .as_i64()
                        .or_else(|| n.as_f64().map(|f| f as i64))
                        .unwrap_or(0)
                        .max(0) as usize,
                    _ => return Some(Ok(Value::Null)),
                };
                if target_len > MAX_PAD_LEN {
                    return Some(Err(Error::CypherExecution(format!(
                        "ERR_PAD_TOO_LARGE: {name}(..., {target_len}) exceeds the \
                         {MAX_PAD_LEN}-character cap; narrow the target length"
                    ))));
                }
                let pad = match pad_val {
                    Value::String(p) if !p.is_empty() => p,
                    Value::String(_) => " ".to_string(),
                    _ => return Some(Ok(Value::Null)),
                };
                let chars: Vec<char> = s.chars().collect();
                if chars.len() >= target_len {
                    return Some(Ok(Value::String(chars[..target_len].iter().collect())));
                }
                let need = target_len - chars.len();
                // Running char-count accumulator instead of re-scanning the
                // growing `padding` buffer with `.chars().count()` on every
                // iteration (was O(n^2) in `need`).
                let pad_char_count = pad.chars().count();
                let mut padding = String::new();
                let mut padding_char_count = 0usize;
                while padding_char_count < need {
                    padding.push_str(&pad);
                    padding_char_count += pad_char_count;
                }
                let padding: String = padding.chars().take(need).collect();
                let result = if name == "lpad" {
                    format!("{padding}{s}")
                } else {
                    format!("{s}{padding}")
                };
                Some(Ok(Value::String(result)))
            }
            // phase4_cypher-parity-quick-wins §1.2 — `normalize(s [,
            // form])` applies Unicode normalization; `form` defaults to
            // NFC and accepts NFC/NFD/NFKC/NFKD (case-insensitive, as
            // Neo4j allows both `'NFC'` and lowercase spellings). An
            // unrecognised form is a query error, not a silent NULL,
            // since it is almost always a caller typo.
            "normalize" => {
                let arg = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                if matches!(arg, Value::Null) {
                    return Some(Ok(Value::Null));
                }
                let s = match arg {
                    Value::String(s) => s,
                    other => {
                        return Some(Err(Error::TypeMismatch {
                            expected: "STRING".to_string(),
                            actual: super::type_name_of(&other).to_string(),
                        }));
                    }
                };
                let form = match args.get(1) {
                    Some(expr) => match self.evaluate_projection_expression(row, context, expr) {
                        Ok(Value::Null) => return Some(Ok(Value::Null)),
                        Ok(Value::String(f)) => f,
                        Ok(other) => {
                            return Some(Err(Error::TypeMismatch {
                                expected: "STRING normal form".to_string(),
                                actual: super::type_name_of(&other).to_string(),
                            }));
                        }
                        Err(e) => return Some(Err(e)),
                    },
                    None => "NFC".to_string(),
                };
                use unicode_normalization::UnicodeNormalization;
                let normalized: String = match form.to_ascii_uppercase().as_str() {
                    "NFC" => s.nfc().collect(),
                    "NFD" => s.nfd().collect(),
                    "NFKC" => s.nfkc().collect(),
                    "NFKD" => s.nfkd().collect(),
                    other => {
                        return Some(Err(Error::CypherExecution(format!(
                            "ERR_INVALID_NORMAL_FORM: normalize() expects NFC, NFD, NFKC, or \
                             NFKD, got `{other}`"
                        ))));
                    }
                };
                Some(Ok(Value::String(normalized)))
            }
            // phase6_opencypher-advanced-types §1 — BYTES family.
            // Uses the `{"_bytes": "<base64>"}` wire shape so
            // the JSON-based runtime stays unchanged. NULL-in →
            // NULL-out across every entry point.
            "bytes" => {
                let arg = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                match arg {
                    Value::Null => Some(Ok(Value::Null)),
                    Value::String(s) => Some(super::super::bytes::bytes_from_vec(s.into_bytes())),
                    other if super::super::bytes::is_bytes_value(&other) => Some(Ok(other)),
                    other => Some(Err(Error::TypeMismatch {
                        expected: "STRING or BYTES".to_string(),
                        actual: super::type_name_of(&other).to_string(),
                    })),
                }
            }
            "bytesfrombase64" => {
                let arg = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                match arg {
                    Value::Null => Some(Ok(Value::Null)),
                    Value::String(s) => {
                        if let Err(e) = super::super::bytes::reject_oversize_base64(&s) {
                            return Some(Err(e));
                        }
                        use base64::Engine as _;
                        use base64::engine::general_purpose::STANDARD as B64;
                        let raw = match B64.decode(&s) {
                            Ok(r) => r,
                            Err(e) => {
                                return Some(Err(Error::CypherExecution(format!(
                                    "ERR_INVALID_BYTES: base64 decode failed: {e}"
                                ))));
                            }
                        };
                        Some(super::super::bytes::bytes_from_vec(raw))
                    }
                    other => Some(Err(Error::TypeMismatch {
                        expected: "STRING".to_string(),
                        actual: super::type_name_of(&other).to_string(),
                    })),
                }
            }
            "bytestobase64" => {
                let arg = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                if matches!(arg, Value::Null) {
                    return Some(Ok(Value::Null));
                }
                let raw = match super::super::bytes::bytes_value_to_vec(&arg) {
                    Ok(r) => r,
                    Err(e) => return Some(Err(e)),
                };
                use base64::Engine as _;
                use base64::engine::general_purpose::STANDARD as B64;
                Some(Ok(Value::String(B64.encode(raw))))
            }
            "bytestohex" => {
                let arg = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                if matches!(arg, Value::Null) {
                    return Some(Ok(Value::Null));
                }
                let raw = match super::super::bytes::bytes_value_to_vec(&arg) {
                    Ok(r) => r,
                    Err(e) => return Some(Err(e)),
                };
                Some(Ok(Value::String(super::super::bytes::to_hex(&raw))))
            }
            "byteslength" => {
                let arg = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                if matches!(arg, Value::Null) {
                    return Some(Ok(Value::Null));
                }
                let raw = match super::super::bytes::bytes_value_to_vec(&arg) {
                    Ok(r) => r,
                    Err(e) => return Some(Err(e)),
                };
                Some(Ok(Value::Number(
                    serde_json::Number::from(raw.len() as i64),
                )))
            }
            "bytesslice" => {
                if args.len() < 3 {
                    return Some(Ok(Value::Null));
                }
                let b = match self.evaluate_projection_expression(row, context, &args[0]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let start_v = match self.evaluate_projection_expression(row, context, &args[1]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let len_v = match self.evaluate_projection_expression(row, context, &args[2]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                if matches!(b, Value::Null)
                    || matches!(start_v, Value::Null)
                    || matches!(len_v, Value::Null)
                {
                    return Some(Ok(Value::Null));
                }
                let raw = match super::super::bytes::bytes_value_to_vec(&b) {
                    Ok(r) => r,
                    Err(e) => return Some(Err(e)),
                };
                let start = match &start_v {
                    Value::Number(n) => n
                        .as_i64()
                        .or_else(|| n.as_f64().map(|f| f as i64))
                        .unwrap_or(0),
                    _ => {
                        return Some(Err(Error::TypeMismatch {
                            expected: "INTEGER".to_string(),
                            actual: super::type_name_of(&start_v).to_string(),
                        }));
                    }
                };
                let len = match &len_v {
                    Value::Number(n) => n
                        .as_i64()
                        .or_else(|| n.as_f64().map(|f| f as i64))
                        .unwrap_or(0),
                    _ => {
                        return Some(Err(Error::TypeMismatch {
                            expected: "INTEGER".to_string(),
                            actual: super::type_name_of(&len_v).to_string(),
                        }));
                    }
                };
                let sliced = super::super::bytes::slice(&raw, start, len);
                Some(super::super::bytes::bytes_from_vec(sliced))
            }
            _ => None,
        }
    }
}
