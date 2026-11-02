//! Inverted index and lexical BM25 retriever with metadata hard-filtering.
//!
//! Provides sub-millisecond, synchronous lexical search across protocol chunks,
//! episodic entries, and grammar templates. Exact technical tokens (`0x84`, `0x56`,
//! `1515`, `3001`, `Timeout`) receive elevated exact-match scoring.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::corpus::{Corpus, ProtocolDocChunk};
use crate::grammar::{GrammarTemplate, TemplateRegistry};

/// Query parameters for the hybrid retriever.
#[derive(Debug, Clone, Default)]
pub struct RetrievalQuery {
    /// Natural language or keyword query string (e.g. `"Philips SICP 0x56 video wall tile positioning"`).
    pub query: String,
    /// Optional hard filter for vendor (e.g. `"philips"`).
    pub vendor: Option<String>,
    /// Optional hard filter for protocol (e.g. `"sicp"`).
    pub protocol: Option<String>,
    /// Optional hard filter for model prefix (e.g. `"BDL"`).
    pub model_prefix: Option<String>,
    /// Optional hard filter for capability (e.g. `"video_wall"`).
    pub capability: Option<String>,
    /// Maximum number of documents to return.
    pub limit: usize,
}

impl RetrievalQuery {
    /// Builds a new query with default limits.
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            vendor: None,
            protocol: None,
            model_prefix: None,
            capability: None,
            limit: 5,
        }
    }

    /// Adds a vendor filter.
    #[must_use]
    pub fn with_vendor(mut self, vendor: impl Into<String>) -> Self {
        self.vendor = Some(vendor.into());
        self
    }

    /// Adds a protocol filter.
    #[must_use]
    pub fn with_protocol(mut self, protocol: impl Into<String>) -> Self {
        self.protocol = Some(protocol.into());
        self
    }

    /// Sets the result limit.
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

/// A scored document or template returned by the retriever.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredMatch<T> {
    /// The retrieved item.
    pub item: T,
    /// BM25 / token match score.
    pub score: f32,
}

/// Tokenizer that splits text into normalized lowercase alphanumeric + hex tokens.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != 'x' && c != 'X')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

/// Inverted index entry pointing to a document ID with term frequency.
#[derive(Debug, Clone)]
struct Posting {
    doc_index: usize,
    term_frequency: usize,
}

/// Lightweight synchronous in-memory BM25 indexer.
#[derive(Debug, Clone, Default)]
pub struct HybridRetriever {
    corpus: Corpus,
    templates: TemplateRegistry,
    // Inverted index for corpus chunks
    corpus_index: HashMap<String, Vec<Posting>>,
    corpus_doc_lens: Vec<usize>,
    avg_corpus_doc_len: f32,
    // Inverted index for grammar templates
    template_index: HashMap<String, Vec<Posting>>,
    template_doc_lens: Vec<usize>,
    avg_template_doc_len: f32,
}

impl HybridRetriever {
    /// Creates a retriever initialized with given corpus and template registry.
    #[must_use]
    pub fn new(corpus: Corpus, templates: TemplateRegistry) -> Self {
        let mut retriever = Self {
            corpus,
            templates,
            corpus_index: HashMap::new(),
            corpus_doc_lens: Vec::new(),
            avg_corpus_doc_len: 0.0,
            template_index: HashMap::new(),
            template_doc_lens: Vec::new(),
            avg_template_doc_len: 0.0,
        };
        retriever.build_indices();
        retriever
    }

    /// Builds inverted indices for BM25 calculation.
    pub fn build_indices(&mut self) {
        // 1. Index Corpus Chunks
        let num_corpus = self.corpus.chunks().len();
        self.corpus_index.clear();
        self.corpus_doc_lens.clear();

        let mut total_corpus_len = 0;
        for (idx, chunk) in self.corpus.chunks().iter().enumerate() {
            let mut text = format!(
                "{} {} {} {} {} {}",
                chunk.id,
                chunk.vendor,
                chunk.protocol,
                chunk.title,
                chunk.content,
                chunk.hex_signatures.join(" ")
            );
            text.push(' ');
            text.push_str(&chunk.tags.join(" "));

            let tokens = tokenize(&text);
            self.corpus_doc_lens.push(tokens.len());
            total_corpus_len += tokens.len();

            let mut freq: HashMap<String, usize> = HashMap::new();
            for token in tokens {
                *freq.entry(token).or_insert(0) += 1;
            }

            for (term, count) in freq {
                self.corpus_index.entry(term).or_default().push(Posting {
                    doc_index: idx,
                    term_frequency: count,
                });
            }
        }
        self.avg_corpus_doc_len = if num_corpus > 0 {
            total_corpus_len as f32 / num_corpus as f32
        } else {
            0.0
        };

        // 2. Index Templates
        let num_templates = self.templates.templates().len();
        self.template_index.clear();
        self.template_doc_lens.clear();

        let mut total_template_len = 0;
        for (idx, tmpl) in self.templates.templates().iter().enumerate() {
            let mut text = format!(
                "{} {} {} {} {}",
                tmpl.id, tmpl.category, tmpl.target, tmpl.title, tmpl.description
            );
            text.push(' ');
            text.push_str(&tmpl.tags.join(" "));

            let tokens = tokenize(&text);
            self.template_doc_lens.push(tokens.len());
            total_template_len += tokens.len();

            let mut freq: HashMap<String, usize> = HashMap::new();
            for token in tokens {
                *freq.entry(token).or_insert(0) += 1;
            }

            for (term, count) in freq {
                self.template_index.entry(term).or_default().push(Posting {
                    doc_index: idx,
                    term_frequency: count,
                });
            }
        }
        self.avg_template_doc_len = if num_templates > 0 {
            total_template_len as f32 / num_templates as f32
        } else {
            0.0
        };
    }

    /// Retrieves corpus chunks matching query with BM25 scoring and metadata filtering.
    #[must_use]
    pub fn retrieve_corpus(&self, query: &RetrievalQuery) -> Vec<ScoredMatch<ProtocolDocChunk>> {
        let query_tokens = tokenize(&query.query);
        let n = self.corpus.chunks().len();
        if n == 0 || query_tokens.is_empty() {
            return Vec::new();
        }

        // BM25 parameters
        let k1: f32 = 1.2;
        let b: f32 = 0.75;

        let mut doc_scores: HashMap<usize, f32> = HashMap::new();
        let query_unique_tokens: HashSet<String> = query_tokens.into_iter().collect();

        for term in query_unique_tokens {
            if let Some(postings) = self.corpus_index.get(&term) {
                let df = postings.len();
                let idf = ((n as f32 - df as f32 + 0.5) / (df as f32 + 0.5) + 1.0).ln();

                for posting in postings {
                    let doc_idx = posting.doc_index;
                    let chunk = &self.corpus.chunks()[doc_idx];

                    // Check metadata filter
                    if !chunk.matches_filter(
                        query.vendor.as_deref(),
                        query.protocol.as_deref(),
                        query.model_prefix.as_deref(),
                        query.capability.as_deref(),
                    ) {
                        continue;
                    }

                    let doc_len = self.corpus_doc_lens[doc_idx] as f32;
                    let tf = posting.term_frequency as f32;
                    let score = idf * (tf * (k1 + 1.0))
                        / (tf + k1 * (1.0 - b + b * (doc_len / self.avg_corpus_doc_len.max(1.0))));

                    // Exact hex or identifier token boost
                    let boost = if term.starts_with("0x") || term.chars().all(|c| c.is_numeric()) {
                        2.5
                    } else {
                        1.0
                    };

                    *doc_scores.entry(doc_idx).or_insert(0.0) += score * boost;
                }
            }
        }

        let mut results: Vec<ScoredMatch<ProtocolDocChunk>> = doc_scores
            .into_iter()
            .map(|(idx, score)| ScoredMatch {
                item: self.corpus.chunks()[idx].clone(),
                score,
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if query.limit > 0 && results.len() > query.limit {
            results.truncate(query.limit);
        }

        results
    }

    /// Retrieves relevant grammar templates matching query.
    #[must_use]
    pub fn retrieve_templates(&self, query: &RetrievalQuery) -> Vec<ScoredMatch<GrammarTemplate>> {
        let query_tokens = tokenize(&query.query);
        let n = self.templates.templates().len();
        if n == 0 || query_tokens.is_empty() {
            return Vec::new();
        }

        let k1: f32 = 1.2;
        let b: f32 = 0.75;

        let mut doc_scores: HashMap<usize, f32> = HashMap::new();
        let query_unique_tokens: HashSet<String> = query_tokens.into_iter().collect();

        for term in query_unique_tokens {
            if let Some(postings) = self.template_index.get(&term) {
                let df = postings.len();
                let idf = ((n as f32 - df as f32 + 0.5) / (df as f32 + 0.5) + 1.0).ln();

                for posting in postings {
                    let doc_idx = posting.doc_index;
                    let tmpl = &self.templates.templates()[doc_idx];

                    if let Some(cap) = &query.capability {
                        if !tmpl.target.eq_ignore_ascii_case(cap) {
                            continue;
                        }
                    }

                    let doc_len = self.template_doc_lens[doc_idx] as f32;
                    let tf = posting.term_frequency as f32;
                    let score = idf * (tf * (k1 + 1.0))
                        / (tf
                            + k1 * (1.0 - b + b * (doc_len / self.avg_template_doc_len.max(1.0))));

                    *doc_scores.entry(doc_idx).or_insert(0.0) += score;
                }
            }
        }

        let mut results: Vec<ScoredMatch<GrammarTemplate>> = doc_scores
            .into_iter()
            .map(|(idx, score)| ScoredMatch {
                item: self.templates.templates()[idx].clone(),
                score,
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if query.limit > 0 && results.len() > query.limit {
            results.truncate(query.limit);
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieves_philips_sicp_videowall_chunk() {
        let corpus = Corpus::default_knowledge_base().unwrap();
        let mut templates = TemplateRegistry::new();
        templates.populate_defaults();

        let retriever = HybridRetriever::new(corpus, templates);
        let query = RetrievalQuery::new("Philips SICP 0x56 video wall tile positioning");

        let matches = retriever.retrieve_corpus(&query);
        assert!(!matches.is_empty(), "must retrieve results for SICP query");
        let top = &matches[0];
        assert_eq!(top.item.id, "philips_sicp_0x56_videowall");
        assert_eq!(top.item.protocol, "sicp");
        assert_eq!(top.item.vendor, "philips");
        assert!(top.item.hex_signatures.contains(&"0x56".to_string()));
    }

    #[test]
    fn retrieves_grammar_templates() {
        let corpus = Corpus::default_knowledge_base().unwrap();
        let mut templates = TemplateRegistry::new();
        templates.populate_defaults();

        let retriever = HybridRetriever::new(corpus, templates);
        let query = RetrievalQuery::new("set up 2x2 video wall scene");

        let matches = retriever.retrieve_templates(&query);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].item.id, "videowall_2x2_scene");
    }
}
