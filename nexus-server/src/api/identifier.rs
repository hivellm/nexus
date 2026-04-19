//! Cypher-identifier validation helper.
//!
//! Several HTTP handlers build Cypher queries by string interpolation:
//! `format!("MATCH (n:{}) RETURN n", user_input)`. If `user_input` is
//! not constrained to the Cypher identifier grammar, a malicious
//! client can escape the pattern — for example by sending
//! `"Person) DETACH DELETE n //"` which closes the node pattern and
//! appends a destructive clause.
//!
//! [`validate_identifier`] enforces the classic identifier rule:
//!
//! ```text
//! [A-Za-z_][A-Za-z0-9_]*
//! ```
//!
//! This matches the openCypher "SymbolicName" production for
//! un-backtick-quoted identifiers. The server interpolates raw, so
//! anything richer would require emitting backticks — which the
//! handlers do not — so the stricter rule is the correct one.
//!
//! The helper is intentionally small and side-effect-free so it can
//! live at every user-input-to-Cypher boundary without measurable
//! latency.

/// Why a candidate identifier was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidIdentifier {
    /// The candidate was empty.
    Empty,
    /// The first character was not `[A-Za-z_]`.
    BadFirstChar {
        /// The candidate value that was rejected (echoed back so the
        /// error message can name the offender, without holding a
        /// reference to the caller's buffer).
        value: String,
        /// The offending first character.
        first: char,
    },
    /// A non-first character was not `[A-Za-z0-9_]`.
    BadBodyChar {
        /// The candidate value that was rejected.
        value: String,
        /// Zero-based byte offset of the offending character.
        index: usize,
        /// The offending character.
        ch: char,
    },
    /// The candidate exceeded [`MAX_IDENTIFIER_LEN`]. Applied *after*
    /// the char-class checks so the error message names the right
    /// reason when a caller sends a long string that also contains
    /// illegal characters.
    TooLong {
        /// The candidate value that was rejected.
        value: String,
        /// Actual length in bytes.
        len: usize,
    },
}

impl std::fmt::Display for InvalidIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "identifier must not be empty"),
            Self::BadFirstChar { value, first } => write!(
                f,
                "identifier {:?} starts with {:?}; the first character must match [A-Za-z_]",
                value, first
            ),
            Self::BadBodyChar { value, index, ch } => write!(
                f,
                "identifier {:?} contains {:?} at byte offset {}; all characters after the first must match [A-Za-z0-9_]",
                value, ch, index
            ),
            Self::TooLong { value, len } => write!(
                f,
                "identifier {:?} is {} bytes; the maximum supported length is {}",
                value, len, MAX_IDENTIFIER_LEN
            ),
        }
    }
}

impl std::error::Error for InvalidIdentifier {}

/// Hard cap on identifier length. The catalog already enforces 255
/// bytes for labels and relationship types; 255 matches that and
/// gives a single error path for overly-long inputs before they hit
/// the Cypher parser.
pub const MAX_IDENTIFIER_LEN: usize = 255;

/// Validate that `s` is a Cypher identifier safe to interpolate into
/// a generated query without backticks. Returns the input verbatim on
/// success so callers can chain:
///
/// ```ignore
/// let safe = validate_identifier(&user_label)?;
/// let query = format!("MATCH (n:{}) RETURN n", safe);
/// ```
pub fn validate_identifier(s: &str) -> Result<&str, InvalidIdentifier> {
    if s.is_empty() {
        return Err(InvalidIdentifier::Empty);
    }

    let mut chars = s.char_indices();
    // SAFETY of unwrap: we just returned on empty, so the iterator
    // has at least one element.
    let (_, first) = chars.next().expect("non-empty checked above");
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(InvalidIdentifier::BadFirstChar {
            value: s.to_string(),
            first,
        });
    }

    for (index, ch) in chars {
        if !(ch.is_ascii_alphanumeric() || ch == '_') {
            return Err(InvalidIdentifier::BadBodyChar {
                value: s.to_string(),
                index,
                ch,
            });
        }
    }

    if s.len() > MAX_IDENTIFIER_LEN {
        return Err(InvalidIdentifier::TooLong {
            value: s.to_string(),
            len: s.len(),
        });
    }

    Ok(s)
}

/// Convenience: validate every identifier in `slice`, returning the
/// first failure. Useful for endpoints that accept a list of labels.
pub fn validate_all<'a, I>(slice: I) -> Result<(), InvalidIdentifier>
where
    I: IntoIterator<Item = &'a str>,
{
    for s in slice {
        validate_identifier(s)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_alpha() {
        assert_eq!(validate_identifier("Person"), Ok("Person"));
    }

    #[test]
    fn accepts_leading_underscore() {
        assert_eq!(validate_identifier("_private"), Ok("_private"));
    }

    #[test]
    fn accepts_alphanumeric_body() {
        assert_eq!(validate_identifier("Node_123"), Ok("Node_123"));
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(validate_identifier(""), Err(InvalidIdentifier::Empty));
    }

    #[test]
    fn rejects_leading_digit() {
        let err = validate_identifier("1Person").unwrap_err();
        assert!(matches!(
            err,
            InvalidIdentifier::BadFirstChar { first: '1', .. }
        ));
    }

    #[test]
    fn rejects_pattern_breakout() {
        // The canonical injection payload: closes the node pattern and
        // appends a destructive clause.
        let err = validate_identifier("Person) DETACH DELETE n //").unwrap_err();
        assert!(matches!(
            err,
            InvalidIdentifier::BadBodyChar { ch: ')', .. }
        ));
    }

    #[test]
    fn rejects_whitespace() {
        let err = validate_identifier("my label").unwrap_err();
        assert!(matches!(
            err,
            InvalidIdentifier::BadBodyChar { ch: ' ', .. }
        ));
    }

    #[test]
    fn rejects_hyphen() {
        // Hyphen is ambiguous with Cypher relationship syntax
        // `-[r]->` so we reject it even though some older validators
        // in the codebase were permissive.
        let err = validate_identifier("my-label").unwrap_err();
        assert!(matches!(
            err,
            InvalidIdentifier::BadBodyChar { ch: '-', .. }
        ));
    }

    #[test]
    fn rejects_overly_long_identifier() {
        let long = "a".repeat(MAX_IDENTIFIER_LEN + 1);
        let err = validate_identifier(&long).unwrap_err();
        assert!(matches!(err, InvalidIdentifier::TooLong { .. }));
    }

    #[test]
    fn accepts_max_length() {
        let ok = "a".repeat(MAX_IDENTIFIER_LEN);
        assert_eq!(validate_identifier(&ok).map(|s| s.len()), Ok(ok.len()));
    }

    #[test]
    fn validate_all_returns_first_failure() {
        let err = validate_all(["Good", "1Bad", "AlsoGood"]).unwrap_err();
        assert!(matches!(
            err,
            InvalidIdentifier::BadFirstChar { first: '1', .. }
        ));
    }

    #[test]
    fn validate_all_accepts_empty_iterator() {
        assert_eq!(validate_all(std::iter::empty::<&str>()), Ok(()));
    }

    #[test]
    fn display_names_the_offender() {
        let msg = validate_identifier("Person) ...").unwrap_err().to_string();
        assert!(
            msg.contains("Person)"),
            "message should echo input: {}",
            msg
        );
    }
}
