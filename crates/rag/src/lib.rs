//! `tv-rs-rag`: In-memory RAG module for hardware quirks, protocol specs,
//! episodic remediation ledgers, and rule/scene AST grammar grounding.
//!
//! Designed for zero-cloud, air-gapped signage deployments. Operates synchronously
//! with bounded memory and zero tokio runtime dependencies.

pub mod assembler;
pub mod corpus;
pub mod grammar;
pub mod ledger;
pub mod retriever;

pub use assembler::{PromptAssembler, RagContextEnvelope};
pub use corpus::{Corpus, ProtocolDocChunk};
pub use grammar::{GrammarTemplate, TemplateRegistry};
pub use ledger::{
    AuditFacts, EmpiricalWeight, EpisodicLedger, RemediationEpisode, RemediationOutcome,
};
pub use retriever::{HybridRetriever, RetrievalQuery, ScoredMatch};
