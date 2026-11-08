//! Opaque-Box E2E Test Suite for tv-rs 0.7.1 Consolidation Refactor.

pub mod harness;
pub mod tier1_feature_coverage;
pub mod tier2_boundary_corner;
pub mod tier3_cross_feature;
pub mod tier4_real_world;

pub fn suite_version() -> &'static str {
    "0.7.1"
}
