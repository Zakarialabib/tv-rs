# E2E Test Infra: tv-rs 0.7.1 Consolidation & Abstraction

## Test Philosophy
- Opaque-box, requirement-driven. No dependency on internal implementation structures.
- Derived from `ORIGINAL_REQUEST.md` and `docs/specdd.md` (SPECDD v3.0).
- Methodology: Category-Partition + Boundary Value Analysis + Pairwise Combinatorial + Real-World Workload Testing.
- Progressive testability: Tier 1 tests verify basic driver instantiation, capabilities, and command dispatch without requiring all complex streaming features.

## Feature Inventory & Test Matrix

| # | Feature | Requirement Source | Tier 1 (Coverage) | Tier 2 (Boundary) | Tier 3 (Cross-Feature) | Tier 4 (Real-World) |
|---|---------|-------------------|:-----------------:|:-----------------:|:---------------------:|:-------------------:|
| 1 | `Driver Trait & Registry` | R1, R2, TV-10 | 5 | 5 | ✓ | ✓ |
| 2 | `Core no_std Boundary` | R3, TV-17 | 5 | 5 | ✓ | ✓ |
| 3 | `Driver Feature Matrix` | R3, TV-18 | 5 | 5 | ✓ | ✓ |
| 4 | `Discovery & Power Drivers` | R1, TV-11 | 5 | 5 | ✓ | ✓ |
| 5 | `Roku, MDC, SICP, IR Drivers` | R1, TV-13 | 5 | 5 | ✓ | ✓ |
| 6 | `Tizen & webOS Drivers` | R1, TV-12 | 5 | 5 | ✓ | ✓ |
| 7 | `Cast & CEC Drivers (Honesty)`| R1, R3, TV-14 | 5 | 5 | ✓ | ✓ |
| 8 | `Observation Push Stream` | R2, TV-15 | 5 | 5 | ✓ | ✓ |
| 9 | `CapabilitySet Registry & Conformance` | R2, TV-16 | 5 | 5 | ✓ | ✓ |
| 10 | `Version Cadences & Docs` | R3, TV-19 | 5 | 5 | ✓ | ✓ |
| 11 | `HIL Verification Matrix` | R3, TV-20 | 5 | 5 | ✓ | ✓ |
| 12 | `Vision Honesty Fix` | R3, TV-21 | 5 | 5 | ✓ | ✓ |

## Test Architecture
- Test runner: `cargo test --test e2e_suite` or `cargo test --workspace`
- Test files location: `tests/e2e/` (or dedicated test targets)
- Pass/Fail semantics: All tests must exit 0 with clean assertions and zero panics.

## Real-World Application Scenarios (Tier 4)
| # | Scenario | Features Exercised | Complexity |
|---|----------|--------------------|------------|
| 1 | Commercial Panel Video-Wall Sync | MDC Driver, Power, Capabilities, Conformance | High |
| 2 | Multi-Vendor Mixed Fleet Discovery & Control | Registry, Tizen, webOS, Roku, Power | High |
| 3 | Real-Time TV Observation & Event Streaming | Observe Stream, Broadcast Ring Buffer, Backpressure | High |
| 4 | Cast & CEC Unified Playback & Power Control | Cast Driver, CEC Driver, TvClient Dispatch | Medium |
| 5 | Legacy Crate Migration Compatibility | Deprecated Shims for Discovery, Power, Tizen, WebOS | Medium |

## Minimum Thresholds
- Tier 1: $\ge 60$ test cases ($12 \text{ features} \times 5$)
- Tier 2: $\ge 60$ test cases ($12 \text{ features} \times 5$)
- Tier 3: $\ge 12$ cross-feature interaction cases
- Tier 4: $\ge 5$ realistic multi-device application scenarios
- **Total Minimum**: $\ge 137$ test cases
