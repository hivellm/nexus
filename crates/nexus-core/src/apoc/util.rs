//! `apoc.util.*` — hashing, UUID, and validation utilities.

use super::{ApocResult, bad_arg, not_found};
use crate::{Error, Result};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use sha2::{Digest, Sha256, Sha512};

pub fn list() -> &'static [&'static str] {
    &[
        "apoc.util.md5",
        "apoc.util.sha1",
        "apoc.util.sha256",
        "apoc.util.sha512",
        "apoc.util.validate",
        "apoc.util.validatePredicate",
        "apoc.util.uuid",
        "apoc.util.compress",
        "apoc.util.decompress",
    ]
}

pub fn dispatch(proc: &str, args: Vec<Value>) -> Result<ApocResult> {
    match proc {
        "md5" => md5_proc(args),
        "sha1" => sha1_proc(args),
        "sha256" => hash_with::<Sha256>(args, "apoc.util.sha256"),
        "sha512" => hash_with::<Sha512>(args, "apoc.util.sha512"),
        "validate" => validate(args),
        "validatePredicate" => validate_predicate(args),
        "uuid" => uuid_v4(),
        "compress" => compress(args),
        "decompress" => decompress(args),
        _ => Err(not_found(&format!("apoc.util.{proc}"))),
    }
}

fn flat_bytes(args: &[Value]) -> Vec<u8> {
    // apoc.util.md5(list) hashes the concatenation of every element's
    // string form (matching APOC's behaviour when `list` holds mixed
    // scalar types).
    let mut out: Vec<u8> = Vec::new();
    if let Some(Value::Array(arr)) = args.first() {
        for v in arr {
            match v {
                Value::String(s) => out.extend_from_slice(s.as_bytes()),
                Value::Null => {}
                other => out.extend_from_slice(other.to_string().as_bytes()),
            }
        }
    } else if let Some(v) = args.first() {
        match v {
            Value::String(s) => out.extend_from_slice(s.as_bytes()),
            Value::Null => {}
            other => out.extend_from_slice(other.to_string().as_bytes()),
        }
    }
    out
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(hex_digit(b >> 4));
        s.push(hex_digit(b & 0x0f));
    }
    s
}

fn hex_digit(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + n - 10) as char,
        _ => '0',
    }
}

/// MD5 (RFC 1321). Kept in-tree so we don't pull in a one-use crate.
fn md5_bytes(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, //
        5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, //
        4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, //
        6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, //
    ];
    const K: [u32; 64] = [
        0xd76a_a478,
        0xe8c7_b756,
        0x2420_70db,
        0xc1bd_ceee,
        0xf57c_0faf,
        0x4787_c62a,
        0xa830_4613,
        0xfd46_9501,
        0x6980_98d8,
        0x8b44_f7af,
        0xffff_5bb1,
        0x895c_d7be,
        0x6b90_1122,
        0xfd98_7193,
        0xa679_438e,
        0x49b4_0821,
        0xf61e_2562,
        0xc040_b340,
        0x265e_5a51,
        0xe9b6_c7aa,
        0xd62f_105d,
        0x0244_1453,
        0xd8a1_e681,
        0xe7d3_fbc8,
        0x21e1_cde6,
        0xc337_07d6,
        0xf4d5_0d87,
        0x455a_14ed,
        0xa9e3_e905,
        0xfcef_a3f8,
        0x676f_02d9,
        0x8d2a_4c8a,
        0xfffa_3942,
        0x8771_f681,
        0x6d9d_6122,
        0xfde5_380c,
        0xa4be_ea44,
        0x4bde_cfa9,
        0xf6bb_4b60,
        0xbebf_bc70,
        0x289b_7ec6,
        0xeaa1_27fa,
        0xd4ef_3085,
        0x0488_1d05,
        0xd9d4_d039,
        0xe6db_99e5,
        0x1fa2_7cf8,
        0xc4ac_5665,
        0xf429_2244,
        0x432a_ff97,
        0xab94_23a7,
        0xfc93_a039,
        0x655b_59c3,
        0x8f0c_cc92,
        0xffef_f47d,
        0x8584_5dd1,
        0x6fa8_7e4f,
        0xfe2c_e6e0,
        0xa301_4314,
        0x4e08_11a1,
        0xf753_7e82,
        0xbd3a_f235,
        0x2ad7_d2bb,
        0xeb86_d391,
    ];

    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    let mut a0: u32 = 0x6745_2301;
    let mut b0: u32 = 0xefcd_ab89;
    let mut c0: u32 = 0x98ba_dcfe;
    let mut d0: u32 = 0x1032_5476;

    for chunk in msg.chunks(64) {
        let mut m = [0u32; 16];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            m[i] = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64usize {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let temp = d;
            d = c;
            c = b;
            b = b.wrapping_add(
                (a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(m[g])).rotate_left(S[i]),
            );
            a = temp;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out
}

/// SHA-1 (FIPS 180-4). In-tree to avoid one-use dependency.
fn sha1_bytes(data: &[u8]) -> [u8; 20] {
    let mut h = [
        0x6745_2301u32,
        0xefcd_ab89u32,
        0x98ba_dcfeu32,
        0x1032_5476u32,
        0xc3d2_e1f0u32,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for i in 0..80usize {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5a82_7999u32),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdcu32),
                _ => (b ^ c ^ d, 0xca62_c1d6u32),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut out = [0u8; 20];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn md5_proc(args: Vec<Value>) -> Result<ApocResult> {
    let bytes = flat_bytes(&args);
    Ok(ApocResult::scalar(Value::String(hex_lower(&md5_bytes(
        &bytes,
    )))))
}

fn sha1_proc(args: Vec<Value>) -> Result<ApocResult> {
    let bytes = flat_bytes(&args);
    Ok(ApocResult::scalar(Value::String(hex_lower(&sha1_bytes(
        &bytes,
    )))))
}

fn hash_with<H: Digest>(args: Vec<Value>, _proc: &str) -> Result<ApocResult> {
    let bytes = flat_bytes(&args);
    let mut hasher = H::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    Ok(ApocResult::scalar(Value::String(hex_lower(
        digest.as_slice(),
    ))))
}

fn validate(args: Vec<Value>) -> Result<ApocResult> {
    // validate(predicate, message, [params]) — throws when predicate is true.
    let pred = args
        .first()
        .and_then(|v| v.as_bool())
        .ok_or_else(|| bad_arg("apoc.util.validate", "arg 0 must be BOOLEAN"))?;
    let msg = args
        .get(1)
        .and_then(|v| v.as_str())
        .unwrap_or("validation failed")
        .to_string();
    if pred {
        return Err(Error::CypherExecution(format!(
            "ERR_VALIDATE_FAILED: {msg}"
        )));
    }
    Ok(ApocResult::scalar(Value::Bool(false)))
}

fn validate_predicate(args: Vec<Value>) -> Result<ApocResult> {
    // Same shape as validate but returns `true` when the predicate
    // passes. Used by APOC guard helpers.
    let pred = args
        .first()
        .and_then(|v| v.as_bool())
        .ok_or_else(|| bad_arg("apoc.util.validatePredicate", "arg 0 must be BOOLEAN"))?;
    let msg = args
        .get(1)
        .and_then(|v| v.as_str())
        .unwrap_or("predicate failed")
        .to_string();
    if !pred {
        return Err(Error::CypherExecution(format!(
            "ERR_VALIDATE_FAILED: {msg}"
        )));
    }
    Ok(ApocResult::scalar(Value::Bool(true)))
}

fn uuid_v4() -> Result<ApocResult> {
    let id = uuid::Uuid::new_v4().to_string();
    Ok(ApocResult::scalar(Value::String(id)))
}

fn compress(args: Vec<Value>) -> Result<ApocResult> {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.util.compress", "arg 0 must be STRING"))?;
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(s.as_bytes())
        .map_err(|e| bad_arg("apoc.util.compress", &format!("gzip failed: {e}")))?;
    let raw = enc
        .finish()
        .map_err(|e| bad_arg("apoc.util.compress", &format!("gzip finish failed: {e}")))?;
    Ok(ApocResult::scalar(Value::String(B64.encode(&raw))))
}

fn decompress(args: Vec<Value>) -> Result<ApocResult> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.util.decompress", "arg 0 must be STRING"))?;
    let raw = B64
        .decode(s)
        .map_err(|e| bad_arg("apoc.util.decompress", &format!("bad base64: {e}")))?;
    let mut dec = GzDecoder::new(&raw[..]);
    let mut out = String::new();
    dec.read_to_string(&mut out)
        .map_err(|e| bad_arg("apoc.util.decompress", &format!("gunzip failed: {e}")))?;
    Ok(ApocResult::scalar(Value::String(out)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(proc: &str, args: Vec<Value>) -> Value {
        dispatch(proc, args).unwrap().rows[0][0].clone()
    }

    #[test]
    fn md5_empty_matches_rfc_1321_test_vector() {
        assert_eq!(
            call("md5", vec![json!([""])]),
            json!("d41d8cd98f00b204e9800998ecf8427e")
        );
    }

    #[test]
    fn md5_abc_matches_rfc_test_vector() {
        assert_eq!(
            call("md5", vec![json!(["abc"])]),
            json!("900150983cd24fb0d6963f7d28e17f72")
        );
    }

    #[test]
    fn sha1_abc_matches_fips_test_vector() {
        assert_eq!(
            call("sha1", vec![json!(["abc"])]),
            json!("a9993e364706816aba3e25717850c26c9cd0d89d")
        );
    }

    #[test]
    fn sha256_abc_matches_fips_test_vector() {
        assert_eq!(
            call("sha256", vec![json!(["abc"])]),
            json!("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
    }

    #[test]
    fn sha512_abc_matches_fips_test_vector() {
        let expected = "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f";
        assert_eq!(call("sha512", vec![json!(["abc"])]), json!(expected));
    }

    #[test]
    fn validate_throws_when_predicate_true() {
        let err = dispatch("validate", vec![json!(true), json!("nope")]).unwrap_err();
        assert!(err.to_string().contains("ERR_VALIDATE_FAILED"));
        assert!(err.to_string().contains("nope"));
    }

    #[test]
    fn validate_passes_when_false() {
        assert_eq!(
            call("validate", vec![json!(false), json!("msg")]),
            json!(false)
        );
    }

    #[test]
    fn uuid_has_36_characters() {
        let id = call("uuid", vec![]);
        assert_eq!(id.as_str().unwrap().len(), 36);
    }

    #[test]
    fn compress_decompress_roundtrip() {
        let enc = call("compress", vec![json!("hello world")]);
        assert_eq!(call("decompress", vec![enc]), json!("hello world"));
    }

    #[test]
    fn md5_hashes_list_concatenation() {
        // APOC semantics: md5 on a list hashes the concatenation of
        // the string-form of every element.
        let out = call("md5", vec![json!(["a", "b", "c"])]);
        let expected = call("md5", vec![json!(["abc"])]);
        assert_eq!(out, expected);
    }
}
