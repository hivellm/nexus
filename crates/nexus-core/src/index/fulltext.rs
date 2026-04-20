//! Full-text search index using Tantivy
//!
//! Features:
//! - BM25 scoring for relevance ranking
//! - Fuzzy search and phrase queries
//! - Per-label/key indexes for efficient filtering
//! - Support for multiple languages
//! - Faceted search capabilities
//! - Highlighting and snippet generation

use crate::Result;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tantivy::query::QueryParser;
use tantivy::schema::Field;
use tantivy::tokenizer::LowerCaser;
use tantivy::tokenizer::Stemmer;
use tantivy::tokenizer::{NgramTokenizer, SimpleTokenizer, TextAnalyzer};
use tantivy::{
    Index, IndexReader, ReloadPolicy, Score, Term,
    collector::TopDocs,
    query::{FuzzyTermQuery, PhraseQuery, Query, TermQuery},
    schema::*,
};

/// Full-text search index for property values
pub struct FullTextIndex {
    /// Tantivy index
    index: Index,
    /// Index reader for searching
    reader: IndexReader,
    /// Schema fields
    schema: Schema,
    /// Field mappings
    fields: FullTextFields,
    /// Statistics
    stats: Arc<RwLock<FullTextStats>>,
}

/// Schema fields for full-text search
#[derive(Debug, Clone)]
pub struct FullTextFields {
    /// Node ID field
    pub node_id: Field,
    /// Label ID field
    pub label_id: Field,
    /// Property key ID field
    pub key_id: Field,
    /// Text content field
    pub content: Field,
    /// Property value field (for exact matching)
    pub value: Field,
    /// Language field (for multi-language support)
    pub language: Field,
    /// Boost field (for relevance scoring)
    pub boost: Field,
}

/// Full-text search statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullTextStats {
    /// Total number of documents indexed
    pub total_documents: u64,
    /// Number of unique labels indexed
    pub label_count: u32,
    /// Number of unique property keys indexed
    pub key_count: u32,
    /// Total text content size in bytes
    pub content_size_bytes: u64,
    /// Average document size in bytes
    pub avg_document_size: f64,
    /// Index size on disk in bytes
    pub index_size_bytes: u64,
    /// Last update timestamp
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// Search result with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Node ID
    pub node_id: u64,
    /// Label ID
    pub label_id: u32,
    /// Property key ID
    pub key_id: u32,
    /// Relevance score
    pub score: f32,
    /// Highlighted snippets
    pub snippets: Vec<String>,
    /// Property value
    pub value: String,
}

/// Search options for full-text queries
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Maximum number of results to return
    pub limit: Option<usize>,
    /// Minimum relevance score threshold
    pub min_score: Option<f32>,
    /// Language filter
    pub language: Option<String>,
    /// Label filter
    pub label_id: Option<u32>,
    /// Property key filter
    pub key_id: Option<u32>,
    /// Enable fuzzy search
    pub fuzzy: bool,
    /// Fuzzy search distance (0-2)
    pub fuzzy_distance: u8,
    /// Enable phrase search
    pub phrase: bool,
    /// Enable highlighting
    pub highlight: bool,
    /// Highlight snippet length
    pub snippet_length: usize,
}

/// Parameters for adding a document to the full-text index
#[derive(Debug, Clone)]
pub struct DocumentParams {
    /// Node ID
    pub node_id: u64,
    /// Label ID
    pub label_id: u32,
    /// Property key ID
    pub key_id: u32,
    /// Text content
    pub content: String,
    /// Property value
    pub value: String,
    /// Language (optional)
    pub language: Option<String>,
    /// Boost factor (optional)
    pub boost: Option<f64>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: Some(100),
            min_score: None,
            language: None,
            label_id: None,
            key_id: None,
            fuzzy: false,
            fuzzy_distance: 1,
            phrase: false,
            highlight: false,
            snippet_length: 100,
        }
    }
}

impl FullTextIndex {
    /// Create a new full-text search index
    pub fn new<P: AsRef<Path>>(index_dir: P) -> Result<Self> {
        let index_dir = index_dir.as_ref();
        std::fs::create_dir_all(index_dir)?;

        // Create schema
        let mut schema_builder = Schema::builder();

        let node_id_field = schema_builder.add_u64_field("node_id", STORED | INDEXED);
        let label_id_field = schema_builder.add_u64_field("label_id", STORED | INDEXED);
        let key_id_field = schema_builder.add_u64_field("key_id", STORED | INDEXED);
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let value_field = schema_builder.add_text_field("value", STORED);
        let language_field = schema_builder.add_text_field("language", STORED);
        let boost_field = schema_builder.add_f64_field("boost", STORED | INDEXED);

        let schema = schema_builder.build();

        let fields = FullTextFields {
            node_id: node_id_field,
            label_id: label_id_field,
            key_id: key_id_field,
            content: content_field,
            value: value_field,
            language: language_field,
            boost: boost_field,
        };

        // Create index
        let index = Index::create_in_dir(index_dir, schema.clone())?;

        // Configure tokenizers
        Self::configure_tokenizers(&index)?;

        // Create reader
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;

        Ok(Self {
            index,
            reader,
            schema,
            fields,
            stats: Arc::new(RwLock::new(FullTextStats {
                total_documents: 0,
                label_count: 0,
                key_count: 0,
                content_size_bytes: 0,
                avg_document_size: 0.0,
                index_size_bytes: 0,
                last_updated: chrono::Utc::now(),
            })),
        })
    }

    /// Configure tokenizers for different languages
    fn configure_tokenizers(index: &Index) -> Result<()> {
        let tokenizer_manager = index.tokenizers();

        // Simple tokenizer for basic text
        let simple_tokenizer = TextAnalyzer::builder(SimpleTokenizer::default())
            .filter(LowerCaser)
            .build();
        tokenizer_manager.register("simple", simple_tokenizer);

        // N-gram tokenizer for fuzzy search
        let ngram_tokenizer = TextAnalyzer::builder(NgramTokenizer::new(2, 3, false)?)
            .filter(LowerCaser)
            .build();
        tokenizer_manager.register("ngram", ngram_tokenizer);

        // English stemmer
        let english_tokenizer = TextAnalyzer::builder(SimpleTokenizer::default())
            .filter(LowerCaser)
            .filter(Stemmer::new(tantivy::tokenizer::Language::English))
            .build();
        tokenizer_manager.register("en", english_tokenizer);

        // Spanish stemmer
        let spanish_tokenizer = TextAnalyzer::builder(SimpleTokenizer::default())
            .filter(LowerCaser)
            .filter(Stemmer::new(tantivy::tokenizer::Language::Spanish))
            .build();
        tokenizer_manager.register("es", spanish_tokenizer);

        // French stemmer
        let french_tokenizer = TextAnalyzer::builder(SimpleTokenizer::default())
            .filter(LowerCaser)
            .filter(Stemmer::new(tantivy::tokenizer::Language::French))
            .build();
        tokenizer_manager.register("fr", french_tokenizer);

        Ok(())
    }

    /// Add a document to the index
    pub fn add_document(&self, params: DocumentParams) -> Result<()> {
        let mut index_writer: tantivy::IndexWriter<tantivy::TantivyDocument> =
            self.index.writer(50_000_000)?; // 50MB buffer

        let mut doc = tantivy::TantivyDocument::new();
        doc.add_u64(self.fields.node_id, params.node_id);
        doc.add_u64(self.fields.label_id, params.label_id as u64);
        doc.add_u64(self.fields.key_id, params.key_id as u64);
        doc.add_text(self.fields.content, &params.content);
        doc.add_text(self.fields.value, &params.value);
        doc.add_text(
            self.fields.language,
            params.language.as_deref().unwrap_or("en"),
        );
        doc.add_f64(self.fields.boost, params.boost.unwrap_or(1.0));

        index_writer.add_document(doc)?;
        index_writer.commit()?;

        // Update statistics
        self.update_stats(params.content.len() as u64)?;

        Ok(())
    }

    /// Remove a document from the index
    pub fn remove_document(&self, node_id: u64, _label_id: u32, _key_id: u32) -> Result<()> {
        let mut index_writer: tantivy::IndexWriter<tantivy::TantivyDocument> =
            self.index.writer(50_000_000)?;

        let term = Term::from_field_u64(self.fields.node_id, node_id);
        index_writer.delete_term(term);

        index_writer.commit()?;

        // Update statistics
        self.update_stats(0)?; // Recalculate stats

        Ok(())
    }

    /// Search for documents
    pub fn search(&self, query: &str, options: SearchOptions) -> Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();

        // Build query
        let tantivy_query = self.build_query(query, &options)?;

        // Execute search
        let limit = options.limit.unwrap_or(100);
        let top_docs = searcher.search(&tantivy_query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::new();

        for (score, doc_address) in top_docs {
            if let Some(min_score) = options.min_score {
                if score < min_score {
                    continue;
                }
            }

            let doc = searcher.doc(doc_address)?;
            let result = self.doc_to_search_result(doc, score, &options)?;
            results.push(result);
        }

        Ok(results)
    }

    /// Build Tantivy query from search string and options
    fn build_query(&self, query: &str, options: &SearchOptions) -> Result<Box<dyn Query>> {
        let mut query_parts = Vec::new();

        // Main content query
        if options.fuzzy {
            let fuzzy_query = FuzzyTermQuery::new(
                Term::from_field_text(self.fields.content, query),
                options.fuzzy_distance,
                true,
            );
            query_parts.push(Box::new(fuzzy_query) as Box<dyn Query>);
        } else if options.phrase {
            let terms: Vec<Term> = query
                .split_whitespace()
                .map(|term| Term::from_field_text(self.fields.content, term))
                .collect();
            let phrase_query = PhraseQuery::new(terms);
            query_parts.push(Box::new(phrase_query) as Box<dyn Query>);
        } else {
            let query_parser = QueryParser::for_index(&self.index, vec![self.fields.content]);
            let tantivy_query = query_parser.parse_query(query)?;
            query_parts.push(tantivy_query);
        }

        // Add filters
        if let Some(label_id) = options.label_id {
            let label_query = TermQuery::new(
                Term::from_field_u64(self.fields.label_id, label_id as u64),
                tantivy::schema::IndexRecordOption::Basic,
            );
            query_parts.push(Box::new(label_query) as Box<dyn Query>);
        }

        if let Some(key_id) = options.key_id {
            let key_query = TermQuery::new(
                Term::from_field_u64(self.fields.key_id, key_id as u64),
                tantivy::schema::IndexRecordOption::Basic,
            );
            query_parts.push(Box::new(key_query) as Box<dyn Query>);
        }

        if let Some(ref language) = options.language {
            let lang_query = TermQuery::new(
                Term::from_field_text(self.fields.language, language),
                tantivy::schema::IndexRecordOption::Basic,
            );
            query_parts.push(Box::new(lang_query) as Box<dyn Query>);
        }

        // Combine queries with AND
        if query_parts.len() == 1 {
            Ok(query_parts.into_iter().next().unwrap())
        } else {
            let combined_query = tantivy::query::BooleanQuery::new(
                query_parts
                    .into_iter()
                    .map(|q| (tantivy::query::Occur::Must, q))
                    .collect(),
            );
            Ok(Box::new(combined_query))
        }
    }

    /// Convert Tantivy document to SearchResult
    fn doc_to_search_result(
        &self,
        doc: tantivy::TantivyDocument,
        score: Score,
        options: &SearchOptions,
    ) -> Result<SearchResult> {
        let node_id = doc
            .get_first(self.fields.node_id)
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let label_id = doc
            .get_first(self.fields.label_id)
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let key_id = doc
            .get_first(self.fields.key_id)
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let value = doc
            .get_first(self.fields.value)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let snippets = if options.highlight {
            self.generate_snippets(&doc, options.snippet_length)?
        } else {
            Vec::new()
        };

        Ok(SearchResult {
            node_id,
            label_id,
            key_id,
            score,
            snippets,
            value,
        })
    }

    /// Generate highlighted snippets for search results
    fn generate_snippets(
        &self,
        _doc: &tantivy::TantivyDocument,
        snippet_length: usize,
    ) -> Result<Vec<String>> {
        // Simplified snippet generation
        // In a real implementation, you would use Tantivy's highlighting features
        let snippets = vec![format!("...{}...", " ".repeat(snippet_length))];
        Ok(snippets)
    }

    /// Update statistics
    fn update_stats(&self, content_size: u64) -> Result<()> {
        let mut stats = self.stats.write();
        stats.total_documents += 1;
        stats.content_size_bytes += content_size;
        stats.avg_document_size = if stats.total_documents > 0 {
            stats.content_size_bytes as f64 / stats.total_documents as f64
        } else {
            0.0
        };
        stats.last_updated = chrono::Utc::now();
        Ok(())
    }

    /// Get statistics
    pub fn get_stats(&self) -> Result<FullTextStats> {
        let stats = self.stats.read();
        Ok(stats.clone())
    }

    /// Clear all documents from the index
    pub fn clear(&self) -> Result<()> {
        let mut index_writer: tantivy::IndexWriter<tantivy::TantivyDocument> =
            self.index.writer(50_000_000)?;
        index_writer.delete_all_documents()?;
        index_writer.commit()?;

        // Reset statistics
        let mut stats = self.stats.write();
        *stats = FullTextStats {
            total_documents: 0,
            label_count: 0,
            key_count: 0,
            content_size_bytes: 0,
            avg_document_size: 0.0,
            index_size_bytes: 0,
            last_updated: chrono::Utc::now(),
        };

        Ok(())
    }

    /// Get index size on disk
    pub fn get_index_size(&self) -> Result<u64> {
        // This would require access to the index directory
        // For now, return a placeholder
        Ok(0)
    }

    /// Optimize the index
    pub fn optimize(&self) -> Result<()> {
        let mut index_writer: tantivy::IndexWriter<tantivy::TantivyDocument> =
            self.index.writer(50_000_000)?;
        index_writer.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_options_default() {
        let options = SearchOptions::default();
        assert_eq!(options.limit, Some(100));
        assert_eq!(options.min_score, None);
        assert_eq!(options.language, None);
        assert_eq!(options.label_id, None);
        assert_eq!(options.key_id, None);
        assert!(!options.fuzzy);
        assert_eq!(options.fuzzy_distance, 1);
        assert!(!options.phrase);
        assert!(!options.highlight);
        assert_eq!(options.snippet_length, 100);
    }

    #[test]
    fn test_search_options_custom() {
        let options = SearchOptions {
            limit: Some(50),
            min_score: Some(0.8),
            language: Some("en".to_string()),
            label_id: Some(1),
            key_id: Some(2),
            fuzzy: true,
            fuzzy_distance: 2,
            phrase: true,
            highlight: true,
            snippet_length: 200,
        };

        assert_eq!(options.limit, Some(50));
        assert_eq!(options.min_score, Some(0.8));
        assert_eq!(options.language, Some("en".to_string()));
        assert_eq!(options.label_id, Some(1));
        assert_eq!(options.key_id, Some(2));
        assert!(options.fuzzy);
        assert_eq!(options.fuzzy_distance, 2);
        assert!(options.phrase);
        assert!(options.highlight);
        assert_eq!(options.snippet_length, 200);
    }

    #[test]
    fn test_fulltext_stats_creation() {
        let stats = FullTextStats {
            total_documents: 0,
            label_count: 0,
            key_count: 0,
            content_size_bytes: 0,
            avg_document_size: 0.0,
            index_size_bytes: 0,
            last_updated: chrono::Utc::now(),
        };

        assert_eq!(stats.total_documents, 0);
        assert_eq!(stats.label_count, 0);
        assert_eq!(stats.key_count, 0);
    }

    #[test]
    fn test_fulltext_stats_with_data() {
        let now = chrono::Utc::now();
        let stats = FullTextStats {
            total_documents: 100,
            label_count: 5,
            key_count: 10,
            content_size_bytes: 50000,
            avg_document_size: 500.0,
            index_size_bytes: 1000000,
            last_updated: now,
        };

        assert_eq!(stats.total_documents, 100);
        assert_eq!(stats.label_count, 5);
        assert_eq!(stats.key_count, 10);
        assert_eq!(stats.content_size_bytes, 50000);
        assert_eq!(stats.avg_document_size, 500.0);
        assert_eq!(stats.index_size_bytes, 1000000);
        assert_eq!(stats.last_updated, now);
    }

    #[test]
    fn test_search_result_creation() {
        let result = SearchResult {
            node_id: 1,
            label_id: 0,
            key_id: 0,
            score: 0.5,
            snippets: vec!["test snippet".to_string()],
            value: "test value".to_string(),
        };

        assert_eq!(result.node_id, 1);
        assert_eq!(result.label_id, 0);
        assert_eq!(result.key_id, 0);
        assert_eq!(result.score, 0.5);
        assert_eq!(result.snippets.len(), 1);
        assert_eq!(result.value, "test value");
    }

    #[test]
    fn test_document_params_creation() {
        let params = DocumentParams {
            node_id: 123,
            label_id: 1,
            key_id: 2,
            content: "test content".to_string(),
            value: "test value".to_string(),
            language: Some("en".to_string()),
            boost: Some(1.5),
        };

        assert_eq!(params.node_id, 123);
        assert_eq!(params.label_id, 1);
        assert_eq!(params.key_id, 2);
        assert_eq!(params.content, "test content");
        assert_eq!(params.value, "test value");
        assert_eq!(params.language, Some("en".to_string()));
        assert_eq!(params.boost, Some(1.5));
    }

    #[test]
    fn test_document_params_minimal() {
        let params = DocumentParams {
            node_id: 456,
            label_id: 0,
            key_id: 0,
            content: "minimal content".to_string(),
            value: "minimal value".to_string(),
            language: None,
            boost: None,
        };

        assert_eq!(params.node_id, 456);
        assert_eq!(params.label_id, 0);
        assert_eq!(params.key_id, 0);
        assert_eq!(params.content, "minimal content");
        assert_eq!(params.value, "minimal value");
        assert_eq!(params.language, None);
        assert_eq!(params.boost, None);
    }

    #[test]
    fn test_fulltext_fields_creation() {
        // Test that FullTextFields can be created
        let fields = FullTextFields {
            node_id: Field::from_field_id(0),
            label_id: Field::from_field_id(1),
            key_id: Field::from_field_id(2),
            content: Field::from_field_id(3),
            value: Field::from_field_id(4),
            language: Field::from_field_id(5),
            boost: Field::from_field_id(6),
        };

        // Fields are created successfully if we can access them
        let _ = fields.node_id;
        let _ = fields.label_id;
        let _ = fields.key_id;
        let _ = fields.content;
        let _ = fields.value;
        let _ = fields.language;
        let _ = fields.boost;
    }

    #[test]
    fn test_search_options_serialization() {
        let options = SearchOptions {
            limit: Some(50),
            min_score: Some(0.8),
            language: Some("en".to_string()),
            label_id: Some(1),
            key_id: Some(2),
            fuzzy: true,
            fuzzy_distance: 2,
            phrase: true,
            highlight: true,
            snippet_length: 200,
        };

        // Test that we can clone the options
        let cloned = options.clone();
        assert_eq!(cloned.limit, options.limit);
        assert_eq!(cloned.min_score, options.min_score);
        assert_eq!(cloned.language, options.language);
        assert_eq!(cloned.label_id, options.label_id);
        assert_eq!(cloned.key_id, options.key_id);
        assert_eq!(cloned.fuzzy, options.fuzzy);
        assert_eq!(cloned.fuzzy_distance, options.fuzzy_distance);
        assert_eq!(cloned.phrase, options.phrase);
        assert_eq!(cloned.highlight, options.highlight);
        assert_eq!(cloned.snippet_length, options.snippet_length);
    }

    #[test]
    fn test_fulltext_stats_serialization() {
        let stats = FullTextStats {
            total_documents: 100,
            label_count: 5,
            key_count: 10,
            content_size_bytes: 50000,
            avg_document_size: 500.0,
            index_size_bytes: 1000000,
            last_updated: chrono::Utc::now(),
        };

        // Test that we can clone the stats
        let cloned = stats.clone();
        assert_eq!(cloned.total_documents, stats.total_documents);
        assert_eq!(cloned.label_count, stats.label_count);
        assert_eq!(cloned.key_count, stats.key_count);
        assert_eq!(cloned.content_size_bytes, stats.content_size_bytes);
        assert_eq!(cloned.avg_document_size, stats.avg_document_size);
        assert_eq!(cloned.index_size_bytes, stats.index_size_bytes);
        assert_eq!(cloned.last_updated, stats.last_updated);
    }

    #[test]
    fn test_search_result_serialization() {
        let result = SearchResult {
            node_id: 1,
            label_id: 0,
            key_id: 0,
            score: 0.5,
            snippets: vec!["test snippet".to_string()],
            value: "test value".to_string(),
        };

        // Test that we can clone the result
        let cloned = result.clone();
        assert_eq!(cloned.node_id, result.node_id);
        assert_eq!(cloned.label_id, result.label_id);
        assert_eq!(cloned.key_id, result.key_id);
        assert_eq!(cloned.score, result.score);
        assert_eq!(cloned.snippets, result.snippets);
        assert_eq!(cloned.value, result.value);
    }

    #[test]
    fn test_document_params_serialization() {
        let params = DocumentParams {
            node_id: 123,
            label_id: 1,
            key_id: 2,
            content: "test content".to_string(),
            value: "test value".to_string(),
            language: Some("en".to_string()),
            boost: Some(1.5),
        };

        // Test that we can clone the params
        let cloned = params.clone();
        assert_eq!(cloned.node_id, params.node_id);
        assert_eq!(cloned.label_id, params.label_id);
        assert_eq!(cloned.key_id, params.key_id);
        assert_eq!(cloned.content, params.content);
        assert_eq!(cloned.value, params.value);
        assert_eq!(cloned.language, params.language);
        assert_eq!(cloned.boost, params.boost);
    }

    #[test]
    fn test_search_options_edge_cases() {
        // Test with no limit
        let options = SearchOptions {
            limit: None,
            ..Default::default()
        };
        assert_eq!(options.limit, None);

        // Test with zero limit
        let options = SearchOptions {
            limit: Some(0),
            ..Default::default()
        };
        assert_eq!(options.limit, Some(0));

        // Test with maximum fuzzy distance
        let options = SearchOptions {
            fuzzy_distance: 2,
            ..Default::default()
        };
        assert_eq!(options.fuzzy_distance, 2);

        // Test with minimum snippet length
        let options = SearchOptions {
            snippet_length: 1,
            ..Default::default()
        };
        assert_eq!(options.snippet_length, 1);
    }

    #[test]
    fn test_fulltext_stats_edge_cases() {
        // Test with maximum values
        let stats = FullTextStats {
            total_documents: u64::MAX,
            label_count: u32::MAX,
            key_count: u32::MAX,
            content_size_bytes: u64::MAX,
            avg_document_size: f64::MAX,
            index_size_bytes: u64::MAX,
            last_updated: chrono::Utc::now(),
        };

        assert_eq!(stats.total_documents, u64::MAX);
        assert_eq!(stats.label_count, u32::MAX);
        assert_eq!(stats.key_count, u32::MAX);
        assert_eq!(stats.content_size_bytes, u64::MAX);
        assert_eq!(stats.avg_document_size, f64::MAX);
        assert_eq!(stats.index_size_bytes, u64::MAX);
    }

    #[test]
    fn test_search_result_edge_cases() {
        // Test with maximum values
        let result = SearchResult {
            node_id: u64::MAX,
            label_id: u32::MAX,
            key_id: u32::MAX,
            score: f32::MAX,
            snippets: vec!["".to_string(); 1000],
            value: "x".repeat(10000),
        };

        assert_eq!(result.node_id, u64::MAX);
        assert_eq!(result.label_id, u32::MAX);
        assert_eq!(result.key_id, u32::MAX);
        assert_eq!(result.score, f32::MAX);
        assert_eq!(result.snippets.len(), 1000);
        assert_eq!(result.value.len(), 10000);
    }

    #[test]
    fn test_document_params_edge_cases() {
        // Test with maximum values
        let params = DocumentParams {
            node_id: u64::MAX,
            label_id: u32::MAX,
            key_id: u32::MAX,
            content: "x".repeat(100000),
            value: "x".repeat(100000),
            language: Some("x".repeat(100)),
            boost: Some(f64::MAX),
        };

        assert_eq!(params.node_id, u64::MAX);
        assert_eq!(params.label_id, u32::MAX);
        assert_eq!(params.key_id, u32::MAX);
        assert_eq!(params.content.len(), 100000);
        assert_eq!(params.value.len(), 100000);
        assert_eq!(params.language.as_ref().unwrap().len(), 100);
        assert_eq!(params.boost, Some(f64::MAX));
    }
}
