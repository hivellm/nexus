//! `apoc.text.*` — string utilities + similarity (phase6 apoc §5).

use super::{ApocResult, bad_arg, not_found};
use crate::{Error, Result};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use regex::Regex;
use serde_json::{Value, json};

pub fn list() -> &'static [&'static str] {
    &[
        "apoc.text.levenshteinDistance",
        "apoc.text.levenshteinSimilarity",
        "apoc.text.jaroWinklerDistance",
        "apoc.text.sorensenDiceSimilarity",
        "apoc.text.hammingDistance",
        "apoc.text.regexGroups",
        "apoc.text.replace",
        "apoc.text.split",
        "apoc.text.phonetic",
        "apoc.text.doubleMetaphone",
        "apoc.text.clean",
        "apoc.text.lpad",
        "apoc.text.rpad",
        "apoc.text.format",
        "apoc.text.base64Encode",
        "apoc.text.base64Decode",
        "apoc.text.camelCase",
        "apoc.text.capitalize",
        "apoc.text.hexValue",
        "apoc.text.byteCount",
    ]
}

pub fn dispatch(proc: &str, args: Vec<Value>) -> Result<ApocResult> {
    match proc {
        "levenshteinDistance" => levenshtein_distance(args),
        "levenshteinSimilarity" => levenshtein_similarity(args),
        "jaroWinklerDistance" => jaro_winkler_distance(args),
        "sorensenDiceSimilarity" => sorensen_dice(args),
        "hammingDistance" => hamming_distance(args),
        "regexGroups" => regex_groups(args),
        "replace" => replace_regex(args),
        "split" => split_regex(args),
        "phonetic" => phonetic(args),
        "doubleMetaphone" => double_metaphone_proc(args),
        "clean" => clean(args),
        "lpad" => lpad(args),
        "rpad" => rpad(args),
        "format" => format_proc(args),
        "base64Encode" => base64_encode(args),
        "base64Decode" => base64_decode(args),
        "camelCase" => camel_case(args),
        "capitalize" => capitalize(args),
        "hexValue" => hex_value(args),
        "byteCount" => byte_count(args),
        _ => Err(not_found(&format!("apoc.text.{proc}"))),
    }
}

fn two_strings(proc: &str, args: &[Value]) -> Result<(String, String)> {
    let a = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg(proc, "expected STRING arg 0"))?;
    let b = args
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg(proc, "expected STRING arg 1"))?;
    Ok((a.to_string(), b.to_string()))
}

fn levenshtein_distance(args: Vec<Value>) -> Result<ApocResult> {
    let (a, b) = two_strings("apoc.text.levenshteinDistance", &args)?;
    let d = strsim::levenshtein(&a, &b) as i64;
    Ok(ApocResult::scalar(Value::Number(d.into())))
}

fn levenshtein_similarity(args: Vec<Value>) -> Result<ApocResult> {
    let (a, b) = two_strings("apoc.text.levenshteinSimilarity", &args)?;
    let d = strsim::levenshtein(&a, &b) as f64;
    let len = a.chars().count().max(b.chars().count()) as f64;
    let sim = if len == 0.0 { 1.0 } else { 1.0 - d / len };
    Ok(ApocResult::scalar(
        serde_json::Number::from_f64(sim)
            .map(Value::Number)
            .unwrap_or(Value::Null),
    ))
}

fn jaro_winkler_distance(args: Vec<Value>) -> Result<ApocResult> {
    let (a, b) = two_strings("apoc.text.jaroWinklerDistance", &args)?;
    let sim = strsim::jaro_winkler(&a, &b);
    Ok(ApocResult::scalar(
        serde_json::Number::from_f64(sim)
            .map(Value::Number)
            .unwrap_or(Value::Null),
    ))
}

fn sorensen_dice(args: Vec<Value>) -> Result<ApocResult> {
    let (a, b) = two_strings("apoc.text.sorensenDiceSimilarity", &args)?;
    let sim = strsim::sorensen_dice(&a, &b);
    Ok(ApocResult::scalar(
        serde_json::Number::from_f64(sim)
            .map(Value::Number)
            .unwrap_or(Value::Null),
    ))
}

fn hamming_distance(args: Vec<Value>) -> Result<ApocResult> {
    let (a, b) = two_strings("apoc.text.hammingDistance", &args)?;
    let d = strsim::hamming(&a, &b)
        .map_err(|e| bad_arg("apoc.text.hammingDistance", &format!("{e}")))?;
    Ok(ApocResult::scalar(Value::Number((d as i64).into())))
}

fn regex_groups(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.regexGroups", "arg 0 must be STRING"))?;
    let pattern = args
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.regexGroups", "arg 1 must be STRING pattern"))?;
    let re = Regex::new(pattern)
        .map_err(|e| bad_arg("apoc.text.regexGroups", &format!("bad regex: {e}")))?;
    let mut out: Vec<Value> = Vec::new();
    for caps in re.captures_iter(s) {
        let groups: Vec<Value> = caps
            .iter()
            .map(|m| {
                m.map(|m| Value::String(m.as_str().to_string()))
                    .unwrap_or(Value::Null)
            })
            .collect();
        out.push(Value::Array(groups));
    }
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn replace_regex(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.replace", "arg 0 must be STRING"))?;
    let pattern = args
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.replace", "arg 1 must be STRING pattern"))?;
    let replacement = args
        .get(2)
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.replace", "arg 2 must be STRING replacement"))?;
    let re = Regex::new(pattern)
        .map_err(|e| bad_arg("apoc.text.replace", &format!("bad regex: {e}")))?;
    Ok(ApocResult::scalar(Value::String(
        re.replace_all(s, replacement).to_string(),
    )))
}

fn split_regex(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.split", "arg 0 must be STRING"))?;
    let pattern = args
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.split", "arg 1 must be STRING pattern"))?;
    let re =
        Regex::new(pattern).map_err(|e| bad_arg("apoc.text.split", &format!("bad regex: {e}")))?;
    let parts: Vec<Value> = re.split(s).map(|p| Value::String(p.to_string())).collect();
    Ok(ApocResult::scalar(Value::Array(parts)))
}

fn phonetic(args: Vec<Value>) -> Result<ApocResult> {
    // American Soundex: first letter kept, next three consonant
    // codes emitted, zero-padded to length 4.
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.phonetic", "arg 0 must be STRING"))?;
    Ok(ApocResult::scalar(Value::String(soundex(s))))
}

fn soundex(input: &str) -> String {
    let mut chars = input.chars().filter(|c| c.is_ascii_alphabetic());
    let first = match chars.next() {
        Some(c) => c.to_ascii_uppercase(),
        None => return String::new(),
    };
    let mut out = String::with_capacity(4);
    out.push(first);
    let mut prev_code = soundex_code(first);
    for c in chars {
        let code = soundex_code(c);
        if code != '0' && code != prev_code {
            out.push(code);
            if out.len() == 4 {
                break;
            }
        }
        if code != '0' {
            prev_code = code;
        }
    }
    while out.len() < 4 {
        out.push('0');
    }
    out
}

fn soundex_code(c: char) -> char {
    match c.to_ascii_uppercase() {
        'B' | 'F' | 'P' | 'V' => '1',
        'C' | 'G' | 'J' | 'K' | 'Q' | 'S' | 'X' | 'Z' => '2',
        'D' | 'T' => '3',
        'L' => '4',
        'M' | 'N' => '5',
        'R' => '6',
        _ => '0',
    }
}

fn double_metaphone_proc(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.doubleMetaphone", "arg 0 must be STRING"))?;
    Ok(ApocResult::scalar(Value::String(metaphone(s))))
}

/// Metaphone algorithm (Lawrence Philips, 1990). Applied ruleset:
///
/// 1. Uppercase ASCII input; ignore non-alphabetic characters.
/// 2. Drop a leading silent letter from
///    `KN`, `GN`, `PN`, `AE`, `WR`.
/// 3. Drop a leading `X` → emit `S`.
/// 4. Walk the string left-to-right, emitting Metaphone codes per
///    position. Characters with no code (vowels after the first
///    position) are skipped.
fn metaphone(input: &str) -> String {
    let raw: Vec<char> = input
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if raw.is_empty() {
        return String::new();
    }

    // Silent leading letters.
    let mut start = 0usize;
    if raw.len() >= 2 {
        let head = [raw[0], raw[1]];
        if matches!(
            head,
            ['K', 'N'] | ['G', 'N'] | ['P', 'N'] | ['A', 'E'] | ['W', 'R']
        ) {
            start = 1;
        }
    }

    let mut out = String::new();
    let at = |i: isize| -> Option<char> {
        if i < 0 {
            None
        } else {
            raw.get(i as usize).copied()
        }
    };

    // Leading X → S (historical convention).
    let mut i = start;
    if at(i as isize) == Some('X') {
        out.push('S');
        i += 1;
    }

    while let Some(&c) = raw.get(i) {
        let prev = at(i as isize - 1);
        let next = at(i as isize + 1);
        let next2 = at(i as isize + 2);

        // Skip duplicate consonants except C.
        if c != 'C' && prev == Some(c) {
            i += 1;
            continue;
        }

        match c {
            'A' | 'E' | 'I' | 'O' | 'U' => {
                if i == start {
                    out.push(c);
                }
            }
            'B' => {
                // Silent B when at end after M (e.g. "dumb").
                if !(i == raw.len() - 1 && prev == Some('M')) {
                    out.push('B');
                }
            }
            'C' => {
                if next == Some('I') && next2 == Some('A') {
                    out.push('X');
                } else if next == Some('H') {
                    out.push('X');
                } else if matches!(next, Some('I') | Some('E') | Some('Y')) {
                    out.push('S');
                } else {
                    out.push('K');
                }
            }
            'D' => {
                if next == Some('G') && matches!(next2, Some('E') | Some('I') | Some('Y')) {
                    out.push('J');
                    i += 2;
                    continue;
                }
                out.push('T');
            }
            'F' | 'J' | 'L' | 'M' | 'N' | 'R' => out.push(c),
            'G' => {
                if next == Some('H') {
                    // GH: dropped between a vowel and end / before consonant; else F.
                    let after = at(i as isize + 2);
                    if matches!(
                        prev,
                        Some('A') | Some('E') | Some('I') | Some('O') | Some('U')
                    ) && (after.is_none() || !at_is_vowel(after))
                    {
                        // dropped
                    } else {
                        out.push('F');
                    }
                    i += 2;
                    continue;
                }
                if next == Some('N') {
                    // silent (GN-) — already handled for leading; trailing GN drops G.
                    i += 1;
                    continue;
                }
                if matches!(next, Some('I') | Some('E') | Some('Y')) {
                    out.push('J');
                } else {
                    out.push('K');
                }
            }
            'H' => {
                // Silent after vowel and before a consonant; otherwise emit H.
                let prev_is_vowel = at_is_vowel(prev);
                let next_is_vowel = at_is_vowel(next);
                if prev_is_vowel && !next_is_vowel {
                    // dropped
                } else {
                    out.push('H');
                }
            }
            'K' => {
                if prev != Some('C') {
                    out.push('K');
                }
            }
            'P' => {
                if next == Some('H') {
                    out.push('F');
                    i += 2;
                    continue;
                }
                out.push('P');
            }
            'Q' => out.push('K'),
            'S' => {
                if next == Some('H') {
                    out.push('X');
                    i += 2;
                    continue;
                }
                if next == Some('I') && (next2 == Some('A') || next2 == Some('O')) {
                    out.push('X');
                } else {
                    out.push('S');
                }
            }
            'T' => {
                if next == Some('H') {
                    out.push('0'); // '0' stands for the θ sound per Philips.
                    i += 2;
                    continue;
                }
                if next == Some('I') && (next2 == Some('A') || next2 == Some('O')) {
                    out.push('X');
                } else {
                    out.push('T');
                }
            }
            'V' => out.push('F'),
            'W' => {
                if at_is_vowel(next) {
                    out.push('W');
                }
            }
            'X' => {
                out.push('K');
                out.push('S');
            }
            'Y' => {
                if at_is_vowel(next) {
                    out.push('Y');
                }
            }
            'Z' => out.push('S'),
            _ => {}
        }
        i += 1;
    }

    out
}

fn at_is_vowel(c: Option<char>) -> bool {
    matches!(
        c,
        Some('A') | Some('E') | Some('I') | Some('O') | Some('U') | Some('Y')
    )
}

fn clean(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.clean", "arg 0 must be STRING"))?;
    let out: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    Ok(ApocResult::scalar(Value::String(out)))
}

fn lpad(args: Vec<Value>) -> Result<ApocResult> {
    let (s, width, pad) = pad_args("apoc.text.lpad", &args)?;
    let n = s.chars().count();
    if n >= width {
        return Ok(ApocResult::scalar(Value::String(s)));
    }
    let missing = width - n;
    let mut out = String::with_capacity(width);
    for _ in 0..missing {
        out.push(pad);
    }
    out.push_str(&s);
    Ok(ApocResult::scalar(Value::String(out)))
}

fn rpad(args: Vec<Value>) -> Result<ApocResult> {
    let (s, width, pad) = pad_args("apoc.text.rpad", &args)?;
    let n = s.chars().count();
    if n >= width {
        return Ok(ApocResult::scalar(Value::String(s)));
    }
    let mut out = s.clone();
    for _ in 0..(width - n) {
        out.push(pad);
    }
    Ok(ApocResult::scalar(Value::String(out)))
}

fn pad_args(proc: &str, args: &[Value]) -> Result<(String, usize, char)> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg(proc, "arg 0 must be STRING"))?
        .to_string();
    let width = args
        .get(1)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| bad_arg(proc, "arg 1 must be INTEGER"))?;
    let pad = args
        .get(2)
        .and_then(|v| v.as_str())
        .and_then(|s| s.chars().next())
        .unwrap_or(' ');
    Ok((s, width.max(0) as usize, pad))
}

fn format_proc(args: Vec<Value>) -> Result<ApocResult> {
    // `{0}` / `{1}` positional or `{name}` named slot substitution.
    let template = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.format", "arg 0 must be STRING"))?;
    let values = args.get(1);
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut key = String::new();
            while let Some(&nc) = chars.peek() {
                chars.next();
                if nc == '}' {
                    break;
                }
                key.push(nc);
            }
            let replacement = match values {
                Some(Value::Array(a)) => {
                    let idx: usize = key.parse().unwrap_or(usize::MAX);
                    a.get(idx).map(json_to_display).unwrap_or_default()
                }
                Some(Value::Object(m)) => m.get(&key).map(json_to_display).unwrap_or_default(),
                _ => String::new(),
            };
            out.push_str(&replacement);
        } else {
            out.push(c);
        }
    }
    Ok(ApocResult::scalar(Value::String(out)))
}

fn json_to_display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn base64_encode(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.base64Encode", "arg 0 must be STRING"))?;
    Ok(ApocResult::scalar(Value::String(B64.encode(s.as_bytes()))))
}

fn base64_decode(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.base64Decode", "arg 0 must be STRING"))?;
    let raw = B64
        .decode(s)
        .map_err(|e| bad_arg("apoc.text.base64Decode", &format!("invalid base64: {e}")))?;
    let decoded = String::from_utf8(raw)
        .map_err(|e| bad_arg("apoc.text.base64Decode", &format!("not valid UTF-8: {e}")))?;
    Ok(ApocResult::scalar(Value::String(decoded)))
}

fn camel_case(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.camelCase", "arg 0 must be STRING"))?;
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for (i, c) in s.chars().enumerate() {
        if !c.is_ascii_alphanumeric() {
            upper = true;
            continue;
        }
        if i == 0 {
            out.push(c.to_ascii_lowercase());
        } else if upper {
            out.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(c.to_ascii_lowercase());
        }
    }
    Ok(ApocResult::scalar(Value::String(out)))
}

fn capitalize(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.capitalize", "arg 0 must be STRING"))?;
    let mut iter = s.chars();
    let out = match iter.next() {
        None => String::new(),
        Some(first) => {
            let mut s = String::with_capacity(s.len());
            s.push(first.to_ascii_uppercase());
            s.extend(iter);
            s
        }
    };
    Ok(ApocResult::scalar(Value::String(out)))
}

fn hex_value(args: Vec<Value>) -> Result<ApocResult> {
    let n = args
        .first()
        .and_then(|v| v.as_i64())
        .ok_or_else(|| bad_arg("apoc.text.hexValue", "arg 0 must be INTEGER"))?;
    Ok(ApocResult::scalar(Value::String(format!("{:X}", n as u64))))
}

fn byte_count(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.text.byteCount", "arg 0 must be STRING"))?;
    Ok(ApocResult::scalar(Value::Number(
        (s.as_bytes().len() as i64).into(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(proc: &str, args: Vec<Value>) -> Value {
        dispatch(proc, args).unwrap().rows[0][0].clone()
    }

    #[test]
    fn levenshtein_distance_kitten_sitting() {
        assert_eq!(
            call(
                "levenshteinDistance",
                vec![json!("kitten"), json!("sitting")]
            ),
            json!(3)
        );
    }

    #[test]
    fn levenshtein_similarity_identical_is_one() {
        assert_eq!(
            call("levenshteinSimilarity", vec![json!("abc"), json!("abc")]),
            json!(1.0)
        );
    }

    #[test]
    fn jaro_winkler_identical_strings() {
        assert_eq!(
            call("jaroWinklerDistance", vec![json!("abc"), json!("abc")]),
            json!(1.0)
        );
    }

    #[test]
    fn soundex_robert_and_rupert_share_code() {
        assert_eq!(call("phonetic", vec![json!("Robert")]), json!("R163"));
        assert_eq!(call("phonetic", vec![json!("Rupert")]), json!("R163"));
    }

    #[test]
    fn double_metaphone_variants_line_up() {
        // Lawrence Philips Metaphone (1990): TH → '0' (θ sound),
        // PH → F, vowels only kept at position 0. "Thompson" walks
        // T-H-O-M-P-S-O-N → 0 M P S N.
        assert_eq!(
            call("doubleMetaphone", vec![json!("Thompson")]),
            json!("0MPSN")
        );
        // "Philip" → P-H-I-L-I-P → F (PH) + L + P = FLP.
        assert_eq!(call("doubleMetaphone", vec![json!("Philip")]), json!("FLP"));
    }

    #[test]
    fn regex_groups_returns_captures() {
        let out = call(
            "regexGroups",
            vec![json!("abc 123 def 456"), json!(r"(\w+) (\d+)")],
        );
        assert_eq!(out[0][0], json!("abc 123"));
        assert_eq!(out[0][1], json!("abc"));
        assert_eq!(out[0][2], json!("123"));
    }

    #[test]
    fn replace_regex_substitutes() {
        assert_eq!(
            call("replace", vec![json!("abc 123"), json!(r"\d+"), json!("X")]),
            json!("abc X")
        );
    }

    #[test]
    fn split_regex_splits() {
        assert_eq!(
            call("split", vec![json!("a,b;c"), json!("[,;]")]),
            json!(["a", "b", "c"])
        );
    }

    #[test]
    fn clean_strips_non_alnum_lowercases() {
        assert_eq!(
            call("clean", vec![json!("Hello, World!")]),
            json!("helloworld")
        );
    }

    #[test]
    fn lpad_pads_left() {
        assert_eq!(
            call("lpad", vec![json!("42"), json!(5), json!("0")]),
            json!("00042")
        );
    }

    #[test]
    fn rpad_pads_right() {
        assert_eq!(
            call("rpad", vec![json!("42"), json!(5), json!("0")]),
            json!("42000")
        );
    }

    #[test]
    fn format_indexed_and_named() {
        assert_eq!(
            call("format", vec![json!("{0} + {1}"), json!(["a", "b"])]),
            json!("a + b")
        );
        assert_eq!(
            call(
                "format",
                vec![json!("hello {name}"), json!({"name": "world"})]
            ),
            json!("hello world")
        );
    }

    #[test]
    fn base64_roundtrip() {
        let enc = call("base64Encode", vec![json!("hello")]);
        assert_eq!(enc, json!("aGVsbG8="));
        assert_eq!(call("base64Decode", vec![enc]), json!("hello"));
    }

    #[test]
    fn camel_case_basic() {
        assert_eq!(
            call("camelCase", vec![json!("hello world foo bar")]),
            json!("helloWorldFooBar")
        );
    }

    #[test]
    fn capitalize_first_letter() {
        assert_eq!(call("capitalize", vec![json!("hello")]), json!("Hello"));
    }

    #[test]
    fn hex_value_uppercase() {
        assert_eq!(call("hexValue", vec![json!(255)]), json!("FF"));
    }

    #[test]
    fn byte_count_utf8() {
        // "á" is 2 bytes in UTF-8 but 1 char.
        assert_eq!(call("byteCount", vec![json!("á")]), json!(2));
    }
}
