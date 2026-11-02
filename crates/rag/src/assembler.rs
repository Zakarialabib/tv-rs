//! Prompt assembly and context envelope generation.
//!
//! Compiles retrieved protocol chunks, episodic weights, and valid command/scene
//! AST templates into a grounded context envelope ready for LLM consumption or
//! natural language rule compilation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::corpus::ProtocolDocChunk;
use crate::grammar::GrammarTemplate;
use crate::ledger::EmpiricalWeight;

/// Structured context envelope prepared for prompt assembly and grounding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagContextEnvelope {
    /// The original user or operator instruction.
    pub query: String,
    /// Relevant protocol documentation and vendor quirks retrieved.
    pub protocol_chunks: Vec<ProtocolDocChunk>,
    /// Empirical healing reliability weights for the target devices/firmware.
    pub empirical_weights: HashMap<String, EmpiricalWeight>,
    /// Valid AST grammar and scene templates grounded for this operation.
    pub grammar_templates: Vec<GrammarTemplate>,
    /// Pre-rendered grounded system context in markdown format.
    pub rendered_markdown: String,
}

/// Assembles retrieved knowledge artifacts into grounded prompts.
#[derive(Debug, Clone, Default)]
pub struct PromptAssembler;

impl PromptAssembler {
    /// Compiles chunks, empirical weights, and grammar templates into a `RagContextEnvelope`.
    #[must_use]
    pub fn assemble(
        query: impl Into<String>,
        protocol_chunks: Vec<ProtocolDocChunk>,
        empirical_weights: HashMap<String, EmpiricalWeight>,
        grammar_templates: Vec<GrammarTemplate>,
    ) -> RagContextEnvelope {
        let query_str = query.into();
        let mut md = String::new();

        md.push_str("# Grounded Context for tv-rs Control & Automation\n\n");
        md.push_str(&format!("**Query:** {}\n\n", query_str));

        if !protocol_chunks.is_empty() {
            md.push_str("## 1. Protocol Specifications & Hardware Quirks\n");
            for chunk in &protocol_chunks {
                md.push_str(&format!(
                    "### [{}] {}\n- **Vendor:** {} | **Protocol:** {}\n- **Hex Signatures:** {:?}\n{}\n\n",
                    chunk.id, chunk.title, chunk.vendor, chunk.protocol, chunk.hex_signatures, chunk.content
                ));
            }
        }

        if !empirical_weights.is_empty() {
            md.push_str("## 2. Empirical Remediation Ledger (Fleet Reliability)\n");
            for (remedy, weight) in &empirical_weights {
                md.push_str(&format!(
                    "- `{}`: success rate {:.1}% ({} successes / {} attempts)\n",
                    remedy,
                    weight.success_rate * 100.0,
                    weight.successes,
                    weight.attempts
                ));
            }
            md.push('\n');
        }

        if !grammar_templates.is_empty() {
            md.push_str("## 3. Grounded AST Grammar Templates (tv-rs-rules)\n");
            for tmpl in &grammar_templates {
                md.push_str(&format!(
                    "### Template: {} ({})\n{}\n```json\n{}\n```\n\n",
                    tmpl.title, tmpl.category, tmpl.description, tmpl.json_schema_payload
                ));
            }
        }

        RagContextEnvelope {
            query: query_str,
            protocol_chunks,
            empirical_weights,
            grammar_templates,
            rendered_markdown: md,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_context_envelope() {
        let envelope =
            PromptAssembler::assemble("configure 2x2 wall", vec![], HashMap::new(), vec![]);
        assert!(envelope.rendered_markdown.contains("configure 2x2 wall"));
    }
}
