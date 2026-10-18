# tv-rs docs

- `matrix.md` — full state matrix (Done / Left / Next), component-level breakdown, external research references (`samsung-tv` crate, REA/JADX, Rust decompilation study), dependency recommendations per crate.
- `skeleton-map.md` — Playbook §1-10 → Phase 1/2/3 mapping (high-level).
- `README.md` — workspace scope (IN/OUT) and build/deploy instructions.

External references incorporated into `matrix.md`:
- `samsung-tv` crate architecture (`tokio-tungstenite`, `native-tls`, `reqwest`) for `tizen`.
- REA / JADX / Apktool (Medium) for Phase 2 auth/token extraction.
- Rust decompilation study (arXiv 2507.18792) confirming `unimplemented!()` is safe.
