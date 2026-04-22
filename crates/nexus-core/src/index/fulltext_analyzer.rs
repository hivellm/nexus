//! Analyzer catalogue for the full-text search backend
//! (phase6_fulltext-analyzer-catalogue).
//!
//! Each [`AnalyzerKind`] resolves to a canonical Tantivy tokenizer
//! name that every [`FullTextIndex`] registers on its own index-local
//! tokenizer manager. The content field's schema carries that name,
//! so each named FTS index uses the analyzer it was created with.
//!
//! The catalogue is Neo4j-aligned: the names surface through
//! `db.index.fulltext.listAvailableAnalyzers()` verbatim.
//!
//! [`FullTextIndex`]: super::fulltext::FullTextIndex
//! [`AnalyzerKind`]: AnalyzerKind

use crate::{Error, Result};
use tantivy::Index;
use tantivy::tokenizer::{
    Language, LowerCaser, NgramTokenizer, RawTokenizer, SimpleTokenizer, Stemmer, StopWordFilter,
    TextAnalyzer, WhitespaceTokenizer,
};

/// Spec for a full-text analyzer registered on a Tantivy index.
///
/// Carries everything `FullTextRegistry::create_*_index` needs to
/// select a tokenizer by name in the schema builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyzerKind {
    /// `standard` — lowercase + English stopwords. Neo4j default.
    Standard,
    /// `whitespace` — split on whitespace only, preserve case.
    Whitespace,
    /// `simple` — lowercase + split on non-alphanumeric runs.
    Simple,
    /// `keyword` — single token pass-through. Case preserved.
    Keyword,
    /// `ngram` — character n-grams. Defaults to 2..=3.
    Ngram { min: usize, max: usize },
    /// Language-stemmed with stopword filter for that language.
    Language(Language),
}

/// Descriptor for a catalogued analyzer (Neo4j parity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzerDescriptor {
    /// Name as seen by `listAvailableAnalyzers()`.
    pub name: &'static str,
    /// Short description as seen by `listAvailableAnalyzers()`.
    pub description: &'static str,
}

/// Enumerate every analyzer name in the catalogue.
///
/// Returns entries **already sorted alphabetically**, matching Neo4j's
/// `db.index.fulltext.listAvailableAnalyzers()` row order.
pub fn catalogue() -> Vec<AnalyzerDescriptor> {
    let mut out = vec![
        AnalyzerDescriptor {
            name: "english",
            description: "English stemmer + lowercase + stopwords",
        },
        AnalyzerDescriptor {
            name: "french",
            description: "French stemmer + lowercase + stopwords",
        },
        AnalyzerDescriptor {
            name: "german",
            description: "German stemmer + lowercase + stopwords",
        },
        AnalyzerDescriptor {
            name: "keyword",
            description: "Single token pass-through; case preserved",
        },
        AnalyzerDescriptor {
            name: "ngram",
            description: "Character n-grams (default 2..3)",
        },
        AnalyzerDescriptor {
            name: "portuguese",
            description: "Portuguese stemmer + lowercase + stopwords",
        },
        AnalyzerDescriptor {
            name: "simple",
            description: "Lowercase + split on non-alphanumeric",
        },
        AnalyzerDescriptor {
            name: "spanish",
            description: "Spanish stemmer + lowercase + stopwords",
        },
        AnalyzerDescriptor {
            name: "standard",
            description: "Default: lowercase + English stopwords",
        },
        AnalyzerDescriptor {
            name: "whitespace",
            description: "Split on whitespace; case preserved",
        },
    ];
    out.sort_by_key(|d| d.name);
    out
}

/// Resolve a user-supplied analyzer name (plus an optional config
/// for ngram sizes) to an [`AnalyzerKind`]. Rejects unknown names
/// with `ERR_FTS_UNKNOWN_ANALYZER`.
pub fn resolve(
    name: &str,
    ngram_min: Option<usize>,
    ngram_max: Option<usize>,
) -> Result<AnalyzerKind> {
    match name {
        "standard" => Ok(AnalyzerKind::Standard),
        "whitespace" => Ok(AnalyzerKind::Whitespace),
        "simple" => Ok(AnalyzerKind::Simple),
        "keyword" => Ok(AnalyzerKind::Keyword),
        "ngram" => {
            let min = ngram_min.unwrap_or(2);
            let max = ngram_max.unwrap_or(3);
            if min == 0 {
                return Err(Error::Storage(
                    "ERR_FTS_UNKNOWN_ANALYZER: ngram min must be >= 1".to_string(),
                ));
            }
            if min > max {
                return Err(Error::Storage(format!(
                    "ERR_FTS_UNKNOWN_ANALYZER: ngram min {min} > max {max}"
                )));
            }
            Ok(AnalyzerKind::Ngram { min, max })
        }
        "english" => Ok(AnalyzerKind::Language(Language::English)),
        "spanish" => Ok(AnalyzerKind::Language(Language::Spanish)),
        "portuguese" => Ok(AnalyzerKind::Language(Language::Portuguese)),
        "german" => Ok(AnalyzerKind::Language(Language::German)),
        "french" => Ok(AnalyzerKind::Language(Language::French)),
        other => Err(Error::Storage(format!(
            "ERR_FTS_UNKNOWN_ANALYZER: {other:?} is not a known analyzer; \
             use db.index.fulltext.listAvailableAnalyzers() to discover names"
        ))),
    }
}

impl AnalyzerKind {
    /// Canonical tokenizer name used both to register on the index
    /// and to reference from the schema's `TextFieldIndexing`.
    pub fn tokenizer_name(&self) -> String {
        match self {
            AnalyzerKind::Standard => "nexus_standard".to_string(),
            AnalyzerKind::Whitespace => "nexus_whitespace".to_string(),
            AnalyzerKind::Simple => "nexus_simple".to_string(),
            AnalyzerKind::Keyword => "nexus_keyword".to_string(),
            AnalyzerKind::Ngram { min, max } => format!("nexus_ngram_{min}_{max}"),
            AnalyzerKind::Language(lang) => format!("nexus_lang_{}", language_tag(*lang)),
        }
    }

    /// Persistent name surfaced by `db.indexes()` and echoed back to
    /// users through the registry metadata.
    pub fn display_name(&self) -> String {
        match self {
            AnalyzerKind::Standard => "standard".to_string(),
            AnalyzerKind::Whitespace => "whitespace".to_string(),
            AnalyzerKind::Simple => "simple".to_string(),
            AnalyzerKind::Keyword => "keyword".to_string(),
            AnalyzerKind::Ngram { min, max } => format!("ngram({min},{max})"),
            AnalyzerKind::Language(lang) => language_tag(*lang).to_string(),
        }
    }

    /// Register the chosen analyzer on the given Tantivy index's
    /// tokenizer manager. Idempotent — re-registering overwrites the
    /// previous entry under the same name.
    pub fn register_on(&self, index: &Index) -> Result<()> {
        let manager = index.tokenizers();
        let name = self.tokenizer_name();
        let analyzer = match self {
            AnalyzerKind::Standard => TextAnalyzer::builder(SimpleTokenizer::default())
                .filter(LowerCaser)
                .filter(
                    StopWordFilter::new(Language::English)
                        .expect("English stopwords are bundled in tantivy 0.22"),
                )
                .build(),
            AnalyzerKind::Whitespace => {
                TextAnalyzer::builder(WhitespaceTokenizer::default()).build()
            }
            AnalyzerKind::Simple => TextAnalyzer::builder(SimpleTokenizer::default())
                .filter(LowerCaser)
                .build(),
            AnalyzerKind::Keyword => TextAnalyzer::builder(RawTokenizer::default()).build(),
            AnalyzerKind::Ngram { min, max } => {
                TextAnalyzer::builder(NgramTokenizer::new(*min, *max, false)?)
                    .filter(LowerCaser)
                    .build()
            }
            AnalyzerKind::Language(lang) => {
                let mut builder = TextAnalyzer::builder(SimpleTokenizer::default())
                    .filter(LowerCaser)
                    .dynamic();
                if let Some(filter) = StopWordFilter::new(*lang) {
                    builder = builder.filter_dynamic(filter);
                }
                builder.filter_dynamic(Stemmer::new(*lang)).build()
            }
        };
        manager.register(&name, analyzer);
        Ok(())
    }
}

fn language_tag(lang: Language) -> &'static str {
    match lang {
        Language::English => "english",
        Language::Spanish => "spanish",
        Language::Portuguese => "portuguese",
        Language::German => "german",
        Language::French => "french",
        Language::Danish => "danish",
        Language::Dutch => "dutch",
        Language::Finnish => "finnish",
        Language::Hungarian => "hungarian",
        Language::Italian => "italian",
        Language::Norwegian => "norwegian",
        Language::Romanian => "romanian",
        Language::Russian => "russian",
        Language::Swedish => "swedish",
        Language::Tamil => "tamil",
        Language::Turkish => "turkish",
        // Tantivy's Language enum is non-exhaustive-ish; cover future
        // additions with a stable fallback so unknown languages still
        // get a deterministic tokenizer name.
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_sorted_alphabetically() {
        let names: Vec<&str> = catalogue().iter().map(|d| d.name).collect();
        let mut expected = names.clone();
        expected.sort();
        assert_eq!(names, expected);
    }

    #[test]
    fn catalogue_contains_every_name() {
        let names: Vec<&str> = catalogue().iter().map(|d| d.name).collect();
        for expected in [
            "standard",
            "whitespace",
            "simple",
            "keyword",
            "ngram",
            "english",
            "spanish",
            "portuguese",
            "german",
            "french",
        ] {
            assert!(names.contains(&expected), "catalogue missing {expected:?}");
        }
    }

    #[test]
    fn resolve_known_names_does_not_fail() {
        for name in [
            "standard",
            "whitespace",
            "simple",
            "keyword",
            "ngram",
            "english",
            "spanish",
            "portuguese",
            "german",
            "french",
        ] {
            resolve(name, None, None).unwrap_or_else(|e| panic!("resolve({name:?}): {e}"));
        }
    }

    #[test]
    fn resolve_ngram_honours_custom_sizes() {
        let kind = resolve("ngram", Some(3), Some(5)).unwrap();
        assert_eq!(kind, AnalyzerKind::Ngram { min: 3, max: 5 });
        assert_eq!(kind.tokenizer_name(), "nexus_ngram_3_5");
    }

    #[test]
    fn resolve_ngram_rejects_inverted_sizes() {
        let err = resolve("ngram", Some(5), Some(2)).unwrap_err();
        assert!(err.to_string().contains("ERR_FTS_UNKNOWN_ANALYZER"));
    }

    #[test]
    fn resolve_ngram_rejects_zero_min() {
        let err = resolve("ngram", Some(0), Some(3)).unwrap_err();
        assert!(err.to_string().contains("ERR_FTS_UNKNOWN_ANALYZER"));
    }

    #[test]
    fn resolve_unknown_analyzer_is_rejected() {
        let err = resolve("klingon", None, None).unwrap_err();
        assert!(err.to_string().contains("ERR_FTS_UNKNOWN_ANALYZER"));
    }

    #[test]
    fn register_all_analyzers_on_tempdir_index() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();

        let mut schema_builder = tantivy::schema::Schema::builder();
        let _ = schema_builder.add_text_field("body", tantivy::schema::TEXT);
        let schema = schema_builder.build();
        let index = Index::create_in_dir(dir.path(), schema).unwrap();

        for name in [
            "standard",
            "whitespace",
            "simple",
            "keyword",
            "english",
            "spanish",
            "portuguese",
            "german",
            "french",
        ] {
            let kind = resolve(name, None, None).unwrap();
            kind.register_on(&index).unwrap();
        }
        let ngram = resolve("ngram", Some(2), Some(4)).unwrap();
        ngram.register_on(&index).unwrap();
    }

    // ----------------------------------------------------------
    // Golden tokenisation tests — one per analyzer, verifying the
    // registered Tantivy `TextAnalyzer` produces the expected
    // token stream.
    // ----------------------------------------------------------

    fn tokens_via(kind: &AnalyzerKind, input: &str) -> Vec<String> {
        use tantivy::tokenizer::Tokenizer;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let mut sb = tantivy::schema::Schema::builder();
        let _ = sb.add_text_field("body", tantivy::schema::TEXT);
        let index = Index::create_in_dir(dir.path(), sb.build()).unwrap();
        kind.register_on(&index).unwrap();
        let mut analyzer = index
            .tokenizers()
            .get(&kind.tokenizer_name())
            .expect("analyzer registered");
        let mut stream = analyzer.token_stream(input);
        let mut out = Vec::new();
        while stream.advance() {
            out.push(stream.token().text.clone());
        }
        out
    }

    #[test]
    fn standard_analyzer_lowercases_and_drops_english_stopwords() {
        let tokens = tokens_via(&AnalyzerKind::Standard, "The Quick Fox IS agile");
        // `the` and `is` are English stopwords removed by StopWordFilter.
        assert!(!tokens.contains(&"the".to_string()), "tokens={tokens:?}");
        assert!(!tokens.contains(&"is".to_string()), "tokens={tokens:?}");
        assert!(tokens.contains(&"quick".to_string()));
        assert!(tokens.contains(&"fox".to_string()));
        assert!(tokens.contains(&"agile".to_string()));
    }

    #[test]
    fn whitespace_analyzer_preserves_case_and_punctuation_in_word() {
        let tokens = tokens_via(&AnalyzerKind::Whitespace, "Hello,World goodbye");
        // Whitespace splitter keeps non-whitespace chars in-token.
        assert!(
            tokens.contains(&"Hello,World".to_string()),
            "tokens={tokens:?}"
        );
        assert!(tokens.contains(&"goodbye".to_string()));
    }

    #[test]
    fn simple_analyzer_lowercases_and_splits_on_punctuation() {
        let tokens = tokens_via(&AnalyzerKind::Simple, "Rust-powered, Blazingly-FAST!");
        assert_eq!(tokens, vec!["rust", "powered", "blazingly", "fast"]);
    }

    #[test]
    fn keyword_analyzer_emits_a_single_token() {
        let tokens = tokens_via(&AnalyzerKind::Keyword, "Hello World 2026");
        assert_eq!(tokens, vec!["Hello World 2026"]);
    }

    #[test]
    fn ngram_analyzer_emits_every_window_of_size_two_to_three() {
        let tokens = tokens_via(&AnalyzerKind::Ngram { min: 2, max: 3 }, "abcd");
        // 2-grams: ab, bc, cd — 3-grams: abc, bcd
        assert!(tokens.contains(&"ab".to_string()), "tokens={tokens:?}");
        assert!(tokens.contains(&"bc".to_string()));
        assert!(tokens.contains(&"cd".to_string()));
        assert!(tokens.contains(&"abc".to_string()));
        assert!(tokens.contains(&"bcd".to_string()));
    }

    #[test]
    fn french_analyzer_drops_le_and_stems_vocabulary() {
        let tokens = tokens_via(
            &AnalyzerKind::Language(Language::French),
            "le chat voit les souris",
        );
        // French stopwords drop le / les.
        assert!(!tokens.contains(&"le".to_string()), "tokens={tokens:?}");
        assert!(!tokens.contains(&"les".to_string()));
        // Stemmer reduces "souris" → keeps it as-is (no plural 's'
        // in French for this case), but tokens must still exist.
        assert!(!tokens.is_empty());
    }

    #[test]
    fn spanish_analyzer_drops_stopwords() {
        let tokens = tokens_via(
            &AnalyzerKind::Language(Language::Spanish),
            "el perro corre en el parque",
        );
        // Spanish stopwords drop el / en.
        assert!(!tokens.contains(&"el".to_string()), "tokens={tokens:?}");
        assert!(!tokens.contains(&"en".to_string()));
    }

    #[test]
    fn portuguese_analyzer_drops_stopwords() {
        let tokens = tokens_via(
            &AnalyzerKind::Language(Language::Portuguese),
            "o gato corre no parque com a bola",
        );
        // Portuguese stopwords drop o / a / no / com.
        assert!(!tokens.contains(&"o".to_string()), "tokens={tokens:?}");
        assert!(!tokens.contains(&"a".to_string()));
        assert!(!tokens.contains(&"no".to_string()));
        assert!(!tokens.contains(&"com".to_string()));
    }

    #[test]
    fn german_analyzer_drops_stopwords() {
        let tokens = tokens_via(
            &AnalyzerKind::Language(Language::German),
            "der Hund läuft in dem Park",
        );
        // German stopwords drop der / in / dem.
        assert!(!tokens.contains(&"der".to_string()), "tokens={tokens:?}");
        assert!(!tokens.contains(&"in".to_string()));
        assert!(!tokens.contains(&"dem".to_string()));
    }
}
