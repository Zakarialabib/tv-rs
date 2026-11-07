# tv-rs-rag

In-memory retrieval-augmented grounding (RAG) module for hardware quirks, protocol specs, episodic remediation history, and declarative rule grammar.

## Architecture

- **Tier A (Corpus)**: Static protocol specs and hardware quirks parsed from `quirks/default.toml`.
- **Tier B (Ledger)**: Bounded ring-buffer episodic remediation log with JSONL persistence and empirical success weight calculations.
- **Tier C (Grammar)**: Declarative rule, scene, and action AST templates for display operations (video walls, input switching, thermal limits).
- **Hybrid Retriever**: BM25 lexical scoring combined with metadata hard-filters and exact hex-token boosts.
- **Air-Gapped**: Pure Rust, zero cloud dependencies, zero external network calls.
