//! `apoc.number.*` — numeric formatting, parsing, and Roman numerals.

use super::{ApocResult, bad_arg, not_found};
use crate::Result;
use serde_json::{Value, json};

pub fn list() -> &'static [&'static str] {
    &[
        "apoc.number.format",
        "apoc.number.parseInt",
        "apoc.number.parseFloat",
        "apoc.number.arabicToRoman",
        "apoc.number.romanToArabic",
        "apoc.number.exact.add",
        "apoc.number.exact.sub",
        "apoc.number.exact.mul",
        "apoc.number.exact.div",
    ]
}

pub fn dispatch(proc: &str, args: Vec<Value>) -> Result<ApocResult> {
    match proc {
        "format" => format_proc(args),
        "parseInt" => parse_int(args),
        "parseFloat" => parse_float(args),
        "arabicToRoman" => arabic_to_roman(args),
        "romanToArabic" => roman_to_arabic(args),
        "exact.add" => exact_binop(args, "add"),
        "exact.sub" => exact_binop(args, "sub"),
        "exact.mul" => exact_binop(args, "mul"),
        "exact.div" => exact_binop(args, "div"),
        _ => Err(not_found(&format!("apoc.number.{proc}"))),
    }
}

fn format_proc(args: Vec<Value>) -> Result<ApocResult> {
    // apoc.number.format(number, precision=2, pattern='#,###.00')
    // Nexus supports the precision parameter; custom pattern falls
    // back to thousands-separated grouping with the requested
    // precision, which covers the common APOC usage.
    let n = match args.first() {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s
            .parse::<f64>()
            .map_err(|e| bad_arg("apoc.number.format", &format!("parse failed: {e}")))?,
        _ => return Err(bad_arg("apoc.number.format", "arg 0 must be number")),
    };
    let precision = args.get(1).and_then(|v| v.as_i64()).unwrap_or(2).max(0) as usize;
    Ok(ApocResult::scalar(Value::String(format_with_commas(
        n, precision,
    ))))
}

fn format_with_commas(n: f64, precision: usize) -> String {
    let negative = n.is_sign_negative();
    let abs = n.abs();
    let rounded = format!("{abs:.precision$}");
    let (int_part, frac_part) = match rounded.find('.') {
        Some(i) => (&rounded[..i], &rounded[i..]),
        None => (rounded.as_str(), ""),
    };
    let grouped = group_thousands(int_part);
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(&grouped);
    out.push_str(frac_part);
    out
}

fn group_thousands(int_part: &str) -> String {
    let bytes = int_part.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, b) in bytes.iter().enumerate() {
        let from_end = len - i;
        if i > 0 && from_end % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

fn parse_int(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.number.parseInt", "arg 0 must be STRING"))?;
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '+')
        .collect();
    Ok(ApocResult::scalar(
        cleaned
            .parse::<i64>()
            .map(|i| Value::Number(i.into()))
            .unwrap_or(Value::Null),
    ))
}

fn parse_float(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.number.parseFloat", "arg 0 must be STRING"))?;
    let cleaned: String = s.chars().filter(|c| *c != ',').collect();
    Ok(ApocResult::scalar(
        cleaned
            .parse::<f64>()
            .ok()
            .and_then(|f| serde_json::Number::from_f64(f).map(Value::Number))
            .unwrap_or(Value::Null),
    ))
}

fn arabic_to_roman(args: Vec<Value>) -> Result<ApocResult> {
    let mut n = args
        .first()
        .and_then(|v| v.as_i64())
        .ok_or_else(|| bad_arg("apoc.number.arabicToRoman", "arg 0 must be INTEGER"))?;
    if !(1..=3999).contains(&n) {
        return Err(bad_arg(
            "apoc.number.arabicToRoman",
            "input outside representable Roman range [1, 3999]",
        ));
    }
    const PAIRS: &[(i64, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, numeral) in PAIRS {
        while n >= *value {
            out.push_str(numeral);
            n -= value;
        }
    }
    Ok(ApocResult::scalar(Value::String(out)))
}

fn roman_to_arabic(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.number.romanToArabic", "arg 0 must be STRING"))?
        .to_ascii_uppercase();
    let mut total: i64 = 0;
    let mut chars: Vec<char> = s.chars().collect();
    chars.push('\0');
    let mut i = 0;
    while i < chars.len() - 1 {
        let cur = roman_digit(chars[i])?;
        let next = roman_digit(chars[i + 1]).unwrap_or(0);
        if cur < next {
            total += next - cur;
            i += 2;
        } else {
            total += cur;
            i += 1;
        }
    }
    Ok(ApocResult::scalar(Value::Number(total.into())))
}

fn roman_digit(c: char) -> Result<i64> {
    match c {
        'I' => Ok(1),
        'V' => Ok(5),
        'X' => Ok(10),
        'L' => Ok(50),
        'C' => Ok(100),
        'D' => Ok(500),
        'M' => Ok(1000),
        '\0' => Ok(0),
        _ => Err(bad_arg(
            "apoc.number.romanToArabic",
            &format!("invalid roman digit {c:?}"),
        )),
    }
}

fn exact_binop(args: Vec<Value>, op: &str) -> Result<ApocResult> {
    let a = args
        .first()
        .ok_or_else(|| bad_arg(&format!("apoc.number.exact.{op}"), "arg 0 missing"))?;
    let b = args
        .get(1)
        .ok_or_else(|| bad_arg(&format!("apoc.number.exact.{op}"), "arg 1 missing"))?;
    let ax = as_i128(a)
        .ok_or_else(|| bad_arg(&format!("apoc.number.exact.{op}"), "arg 0 not integer-like"))?;
    let bx = as_i128(b)
        .ok_or_else(|| bad_arg(&format!("apoc.number.exact.{op}"), "arg 1 not integer-like"))?;
    let result: i128 = match op {
        "add" => ax.checked_add(bx),
        "sub" => ax.checked_sub(bx),
        "mul" => ax.checked_mul(bx),
        "div" => {
            if bx == 0 {
                return Err(bad_arg(
                    &format!("apoc.number.exact.{op}"),
                    "division by zero",
                ));
            }
            Some(ax / bx)
        }
        _ => None,
    }
    .ok_or_else(|| bad_arg(&format!("apoc.number.exact.{op}"), "overflow"))?;
    // Exact arithmetic returns a STRING so callers can round-trip
    // large integers without losing precision through f64.
    Ok(ApocResult::scalar(Value::String(result.to_string())))
}

fn as_i128(v: &Value) -> Option<i128> {
    match v {
        Value::Number(n) => n.as_i64().map(|i| i as i128),
        Value::String(s) => s.parse::<i128>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(proc: &str, args: Vec<Value>) -> Value {
        dispatch(proc, args).unwrap().rows[0][0].clone()
    }

    #[test]
    fn format_thousands_separator() {
        assert_eq!(
            call("format", vec![json!(1234567.89)]),
            json!("1,234,567.89")
        );
    }

    #[test]
    fn format_precision_zero() {
        assert_eq!(
            call("format", vec![json!(1234.567), json!(0)]),
            json!("1,235")
        );
    }

    #[test]
    fn format_negative_preserves_sign() {
        assert_eq!(call("format", vec![json!(-1234.5)]), json!("-1,234.50"));
    }

    #[test]
    fn parse_int_strips_commas() {
        assert_eq!(call("parseInt", vec![json!("1,234,567")]), json!(1_234_567));
    }

    #[test]
    fn parse_float_strips_commas() {
        assert_eq!(call("parseFloat", vec![json!("1,234.5")]), json!(1234.5));
    }

    #[test]
    fn arabic_to_roman_classic_examples() {
        assert_eq!(call("arabicToRoman", vec![json!(1)]), json!("I"));
        assert_eq!(call("arabicToRoman", vec![json!(4)]), json!("IV"));
        assert_eq!(call("arabicToRoman", vec![json!(9)]), json!("IX"));
        assert_eq!(call("arabicToRoman", vec![json!(1994)]), json!("MCMXCIV"));
        assert_eq!(call("arabicToRoman", vec![json!(3999)]), json!("MMMCMXCIX"));
    }

    #[test]
    fn arabic_to_roman_out_of_range_rejected() {
        assert!(dispatch("arabicToRoman", vec![json!(0)]).is_err());
        assert!(dispatch("arabicToRoman", vec![json!(4000)]).is_err());
    }

    #[test]
    fn roman_to_arabic_roundtrip() {
        assert_eq!(call("romanToArabic", vec![json!("IV")]), json!(4));
        assert_eq!(call("romanToArabic", vec![json!("MCMXCIV")]), json!(1994));
    }

    #[test]
    fn exact_add_sub_mul() {
        assert_eq!(
            call("exact.add", vec![json!(9_000_000_000i64), json!(1)]),
            json!("9000000001")
        );
        assert_eq!(call("exact.sub", vec![json!("10"), json!("7")]), json!("3"));
        assert_eq!(
            call("exact.mul", vec![json!(2_000_000_000i64), json!(3)]),
            json!("6000000000")
        );
    }

    #[test]
    fn exact_div_rejects_zero() {
        assert!(dispatch("exact.div", vec![json!(1), json!(0)]).is_err());
    }

    #[test]
    fn exact_overflow_detected() {
        // `i128::MAX` times 2 overflows i128 and surfaces as an error.
        let max = i128::MAX.to_string();
        let err = dispatch("exact.mul", vec![json!(max), json!("2")]).unwrap_err();
        assert!(err.to_string().contains("overflow"));
    }
}
