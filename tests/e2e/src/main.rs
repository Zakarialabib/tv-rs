//! Dedicated CLI E2E Test Runner for tv-rs 0.7.1 Refactor Verification.

use std::future::Future;
use std::pin::Pin;
use std::process::ExitCode;
use std::time::Instant;

use tv_rs_e2e::tier1_feature_coverage::*;
use tv_rs_e2e::tier2_boundary_corner::*;
use tv_rs_e2e::tier3_cross_feature::*;
use tv_rs_e2e::tier4_real_world::*;

type TestFn = fn() -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

struct TestCase {
    name: &'static str,
    tier: u8,
    feature: u8,
    func: TestFn,
}

macro_rules! reg {
    ($name:literal, $tier:expr, $feat:expr, $func:path) => {
        TestCase {
            name: $name,
            tier: $tier,
            feature: $feat,
            func: || Box::pin($func()),
        }
    };
}

fn register_all_tests() -> Vec<TestCase> {
    vec![
        // ==========================================
        // TIER 1: Feature Coverage (60 tests)
        // ==========================================
        // Feature 1: Driver Trait & Registry
        reg!("T1.F01.01: Driver Trait Vendor Dispatch", 1, 1, feat01_driver_trait_registry::test_01_driver_trait_vendor_dispatch),
        reg!("T1.F01.02: Driver Capabilities Reporting", 1, 1, feat01_driver_trait_registry::test_02_driver_capabilities_reporting),
        reg!("T1.F01.03: Driver Connect Returns Handle", 1, 1, feat01_driver_trait_registry::test_03_driver_connect_returns_handle),
        reg!("T1.F01.04: Driver Send Command Ack", 1, 1, feat01_driver_trait_registry::test_04_driver_send_command_ack),
        reg!("T1.F01.05: Registry Enumerates Enabled Drivers", 1, 1, feat01_driver_trait_registry::test_05_registry_enumerates_enabled_drivers),

        // Feature 2: Core no_std Boundary
        reg!("T1.F02.01: Core Manifest Zero Tokio Dependency", 1, 2, feat02_core_nostd_boundary::test_01_tv_rs_core_manifest_has_no_tokio_dependency),
        reg!("T1.F02.02: Core Declares Alloc Feature", 1, 2, feat02_core_nostd_boundary::test_02_tv_rs_core_declares_alloc_feature),
        reg!("T1.F02.03: Core Types Instantiable Without Runtime", 1, 2, feat02_core_nostd_boundary::test_03_core_types_instantiable_without_runtime),
        reg!("T1.F02.04: Core Device Handle Erasure", 1, 2, feat02_core_nostd_boundary::test_04_core_device_handle_erasure),
        reg!("T1.F02.05: Core Error Taxonomy no_std Compatible", 1, 2, feat02_core_nostd_boundary::test_05_core_error_taxonomy_no_std_compatible),

        // Feature 3: Driver Feature Matrix
        reg!("T1.F03.01: Workflow Defines Ten Driver Features", 1, 3, feat03_driver_feature_matrix::test_01_workflow_defines_all_ten_driver_features),
        reg!("T1.F03.02: Drivers Manifest Defines Feature Flags", 1, 3, feat03_driver_feature_matrix::test_02_drivers_manifest_defines_feature_flags),
        reg!("T1.F03.03: No Default Features Configuration", 1, 3, feat03_driver_feature_matrix::test_03_no_default_features_configuration),
        reg!("T1.F03.04: All Features Configuration", 1, 3, feat03_driver_feature_matrix::test_04_all_features_configuration),
        reg!("T1.F03.05: Feature Independence", 1, 3, feat03_driver_feature_matrix::test_05_feature_independence),

        // Feature 4: Discovery & Power Drivers
        reg!("T1.F04.01: SSDP M-SEARCH Datagram Construction", 1, 4, feat04_discovery_power_drivers::test_01_ssdp_msearch_datagram_construction),
        reg!("T1.F04.02: UPnP Location Descriptor Parsing", 1, 4, feat04_discovery_power_drivers::test_02_upnp_location_descriptor_parsing),
        reg!("T1.F04.03: WoL Magic Packet Payload Structure", 1, 4, feat04_discovery_power_drivers::test_03_wol_magic_packet_payload_structure),
        reg!("T1.F04.04: TCP Power Probe Port Detection", 1, 4, feat04_discovery_power_drivers::test_04_tcp_power_probe_port_detection),
        reg!("T1.F04.05: Deprecated Shims Reexport Discovery Power", 1, 4, feat04_discovery_power_drivers::test_05_deprecated_shims_reexport_discovery_power),

        // Feature 5: Consumer & Panel Drivers
        reg!("T1.F05.01: Roku ECP Keypress URL Encoding", 1, 5, feat05_consumer_panel_drivers::test_01_roku_ecp_keypress_url_encoding),
        reg!("T1.F05.02: MDC Packet Framing & Checksum", 1, 5, feat05_consumer_panel_drivers::test_02_mdc_packet_framing_and_checksum),
        reg!("T1.F05.03: MDC Video Wall Command Structure", 1, 5, feat05_consumer_panel_drivers::test_03_mdc_video_wall_command_structure),
        reg!("T1.F05.04: SICP Command Packet Structure", 1, 5, feat05_consumer_panel_drivers::test_04_sicp_command_packet_structure),
        reg!("T1.F05.05: Sony IR IRCC Payload Generation", 1, 5, feat05_consumer_panel_drivers::test_05_ir_ircc_payload_generation),

        // Feature 6: Tizen & webOS Drivers
        reg!("T1.F06.01: Tizen WS Pairing Frame Schema", 1, 6, feat06_tizen_webos_drivers::test_01_tizen_ws_pairing_frame_schema),
        reg!("T1.F06.02: Tizen Token Persistence Lifecycle", 1, 6, feat06_tizen_webos_drivers::test_02_tizen_token_persistence_lifecycle),
        reg!("T1.F06.03: webOS SSAP Registration Payload", 1, 6, feat06_tizen_webos_drivers::test_03_webos_ssap_registration_payload),
        reg!("T1.F06.04: webOS Client-Key Persistence", 1, 6, feat06_tizen_webos_drivers::test_04_webos_client_key_persistence),
        reg!("T1.F06.05: Error Mapping to TvError", 1, 6, feat06_tizen_webos_drivers::test_05_error_mapping_to_tv_error),

        // Feature 7: Cast & CEC Drivers (Honesty)
        reg!("T1.F07.01: Cast Driver Implements Driver Trait", 1, 7, feat07_cast_cec_honesty::test_01_cast_driver_implements_driver_trait),
        reg!("T1.F07.02: Cast Media Status Query Dispatch", 1, 7, feat07_cast_cec_honesty::test_02_cast_media_status_query_dispatch),
        reg!("T1.F07.03: Cast Transport Controls", 1, 7, feat07_cast_cec_honesty::test_03_cast_transport_controls),
        reg!("T1.F07.04: CEC Client Implements Driver Trait", 1, 7, feat07_cast_cec_honesty::test_04_cec_client_implements_driver_trait),
        reg!("T1.F07.05: CEC Power & Active Source", 1, 7, feat07_cast_cec_honesty::test_05_cec_power_and_active_source),

        // Feature 8: Observation Push Stream
        reg!("T1.F08.01: Observation Stream Creation", 1, 8, feat08_observation_push_stream::test_01_observation_stream_creation),
        reg!("T1.F08.02: Event Dispatch to Subscribers", 1, 8, feat08_observation_push_stream::test_02_event_dispatch_to_subscribers),
        reg!("T1.F08.03: Multi-Subscriber Fanout", 1, 8, feat08_observation_push_stream::test_03_multi_subscriber_fanout),
        reg!("T1.F08.04: Stream Ordering Guarantee", 1, 8, feat08_observation_push_stream::test_04_stream_ordering_guarantee),
        reg!("T1.F08.05: TvClient Observe Delegates to Driver", 1, 8, feat08_observation_push_stream::test_05_tv_client_observe_delegates_to_driver),

        // Feature 9: CapabilitySet Registry & Conformance
        reg!("T1.F09.01: CapabilitySet From Driver", 1, 9, feat09_capability_conformance::test_01_capability_set_from_driver),
        reg!("T1.F09.02: Capability Registry Registration", 1, 9, feat09_capability_conformance::test_02_capability_registry_registration),
        reg!("T1.F09.03: Conformance Case Execution", 1, 9, feat09_capability_conformance::test_03_conformance_case_execution),
        reg!("T1.F09.04: Capability Subset Query", 1, 9, feat09_capability_conformance::test_04_capability_subset_query),
        reg!("T1.F09.05: Capability Union & Aggregation", 1, 9, feat09_capability_conformance::test_05_capability_union_and_aggregation),

        // Feature 10: Version Cadences & Docs
        reg!("T1.F10.01: Versioning Policy Doc Exists", 1, 10, feat10_version_cadences::test_01_versioning_policy_doc_exists),
        reg!("T1.F10.02: tv-rs-core 1.0-Track Cadence", 1, 10, feat10_version_cadences::test_02_tv_rs_core_is_on_one_zero_track),
        reg!("T1.F10.03: tv-rs-drivers Follows Facade", 1, 10, feat10_version_cadences::test_03_tv_rs_drivers_follows_facade),
        reg!("T1.F10.04: Standalone Crates Independent Cadence", 1, 10, feat10_version_cadences::test_04_standalone_crates_have_independent_cadence),
        reg!("T1.F10.05: Changelog Structure Per Crate", 1, 10, feat10_version_cadences::test_05_changelog_structure_per_crate),

        // Feature 11: HIL Verification Matrix
        reg!("T1.F11.01: HIL Results Document Exists", 1, 11, feat11_hil_verification_matrix::test_01_hil_results_document_exists),
        reg!("T1.F11.02: HIL Table Covers Drivers", 1, 11, feat11_hil_verification_matrix::test_02_hil_table_covers_all_ten_drivers),
        reg!("T1.F11.03: HIL Table Columns Complete", 1, 11, feat11_hil_verification_matrix::test_03_hil_table_columns_complete),
        reg!("T1.F11.04: HIL Feature Gate in tv-rs", 1, 11, feat11_hil_verification_matrix::test_04_hil_feature_gate_in_tv_rs),
        reg!("T1.F11.05: HIL Harness References Documented Devices", 1, 11, feat11_hil_verification_matrix::test_05_hil_test_harness_references_documented_devices),

        // Feature 12: Vision Honesty Fix
        reg!("T1.F12.01: Synthetic Frame Removed From Facade", 1, 12, feat12_vision_honesty_fix::test_01_vision_synthetic_frame_removed_from_public_facade),
        reg!("T1.F12.02: Vision Publish Flag False", 1, 12, feat12_vision_honesty_fix::test_02_vision_crate_publish_flag_set_false),
        reg!("T1.F12.03: Specdd Honesty Table Records TV-21", 1, 12, feat12_vision_honesty_fix::test_03_specdd_honesty_table_records_vision_task),
        reg!("T1.F12.04: Stream Module Honesty Comment", 1, 12, feat12_vision_honesty_fix::test_04_stream_module_honesty_comment),
        reg!("T1.F12.05: Video Decoder Interface Defined", 1, 12, feat12_vision_honesty_fix::test_05_video_frame_decoder_interface_defined),

        // ==========================================
        // TIER 2: Boundary & Corner Cases (60 tests)
        // ==========================================
        // Feature 1 Boundary
        reg!("T2.F01.01: Unsupported Command Typed Error", 2, 1, boundary01_driver_trait_registry::test_01_driver_send_unsupported_command_returns_typed_error),
        reg!("T2.F01.02: Connect Invalid Target IP", 2, 1, boundary01_driver_trait_registry::test_02_driver_connect_invalid_target_ip),
        reg!("T2.F01.03: Registry Lookup Unknown Vendor", 2, 1, boundary01_driver_trait_registry::test_03_registry_lookup_unknown_vendor),
        reg!("T2.F01.04: Handle Drop Safety", 2, 1, boundary01_driver_trait_registry::test_04_driver_handle_dropped_safely),
        reg!("T2.F01.05: Driver Reconnect Resilience", 2, 1, boundary01_driver_trait_registry::test_05_driver_reconnect_resilience),

        // Feature 2 Boundary
        reg!("T2.F02.01: Zero Tokio Nodes in Core Dependencies", 2, 2, boundary02_core_nostd_boundary::test_01_cargo_tree_audit_core_has_zero_tokio_nodes),
        reg!("T2.F02.02: Command ID Boundary Values", 2, 2, boundary02_core_nostd_boundary::test_02_core_command_id_boundary_values),
        reg!("T2.F02.03: CapabilitySet All Flags", 2, 2, boundary02_core_nostd_boundary::test_03_core_capability_set_all_flags),
        reg!("T2.F02.04: Core Error Matching Coverage", 2, 2, boundary02_core_nostd_boundary::test_04_core_error_clone_and_matching),
        reg!("T2.F02.05: Thumbv7em Target Gate Sanity", 2, 2, boundary02_core_nostd_boundary::test_05_thumbv7em_target_cfg_sanity),

        // Feature 3 Boundary
        reg!("T2.F03.01: Missing Feature Fails Cleanly", 2, 3, boundary03_driver_feature_matrix::test_01_missing_feature_fails_cleanly),
        reg!("T2.F03.02: Conflicting TLS Backends Prevented", 2, 3, boundary03_driver_feature_matrix::test_02_conflicting_tls_backends_prevented),
        reg!("T2.F03.03: Matrix Combinations Cardinality", 2, 3, boundary03_driver_feature_matrix::test_03_feature_matrix_all_combinations_cardinality),
        reg!("T2.F03.04: Empty Registry on Zero Features", 2, 3, boundary03_driver_feature_matrix::test_04_empty_registry_when_all_features_disabled),
        reg!("T2.F03.05: Circular Feature Prohibition", 2, 3, boundary03_driver_feature_matrix::test_05_circular_feature_dependency_prohibition),

        // Feature 4 Boundary
        reg!("T2.F04.01: WoL Invalid MAC Address Format", 2, 4, boundary04_discovery_power_drivers::test_01_wol_invalid_mac_address_format),
        reg!("T2.F04.02: SSDP Truncated Response Handling", 2, 4, boundary04_discovery_power_drivers::test_02_ssdp_truncated_response_handling),
        reg!("T2.F04.03: UPnP XML Malformed Entity Injection", 2, 4, boundary04_discovery_power_drivers::test_03_upnp_xml_malformed_entity_injection),
        reg!("T2.F04.04: TCP Power Probe Timeout Behavior", 2, 4, boundary04_discovery_power_drivers::test_04_tcp_power_probe_timeout_behavior),
        reg!("T2.F04.05: Broadcast Address Edge Cases", 2, 4, boundary04_discovery_power_drivers::test_05_broadcast_address_edge_cases),

        // Feature 5 Boundary
        reg!("T2.F05.01: MDC Checksum Mismatch Rejection", 2, 5, boundary05_consumer_panel_drivers::test_01_mdc_checksum_mismatch_rejection),
        reg!("T2.F05.02: MDC Broadcast Display ID 0xFE", 2, 5, boundary05_consumer_panel_drivers::test_02_mdc_broadcast_display_id_0xfe),
        reg!("T2.F05.03: Roku ECP Special Key Escaping", 2, 5, boundary05_consumer_panel_drivers::test_03_roku_ecp_empty_or_special_key),
        reg!("T2.F05.04: SICP NAK Response Parsing", 2, 5, boundary05_consumer_panel_drivers::test_04_sicp_nak_response_parsing),
        reg!("T2.F05.05: Sony IRCC Invalid Code Handling", 2, 5, boundary05_consumer_panel_drivers::test_05_ir_invalid_ircc_code_handling),

        // Feature 6 Boundary
        reg!("T2.F06.01: Tizen Pairing Rejected by User", 2, 6, boundary06_tizen_webos_drivers::test_01_tizen_pairing_rejected_by_user),
        reg!("T2.F06.02: webOS Invalid Key Forces Re-registration", 2, 6, boundary06_tizen_webos_drivers::test_02_webos_invalid_client_key_forces_reregistration),
        reg!("T2.F06.03: WS Frame Fragmentation & UTF8", 2, 6, boundary06_tizen_webos_drivers::test_03_ws_frame_fragmentation_and_utf8),
        reg!("T2.F06.04: Tizen Port Fallback 8002 to 8001", 2, 6, boundary06_tizen_webos_drivers::test_04_tizen_port_fallback_8002_to_8001),
        reg!("T2.F06.05: webOS Luna Error Code Mapping", 2, 6, boundary06_tizen_webos_drivers::test_05_webos_luna_error_code_mapping),

        // Feature 7 Boundary
        reg!("T2.F07.01: Cast Removes UnsupportedDevice Shim", 2, 7, boundary07_cast_cec_honesty::test_01_cast_removes_unsupported_device_shim),
        reg!("T2.F07.02: CEC Subprocess Not Found Clean Error", 2, 7, boundary07_cast_cec_honesty::test_02_cec_subprocess_not_found_clean_error),
        reg!("T2.F07.03: Cast TLS EKE2 Handshake Timeout", 2, 7, boundary07_cast_cec_honesty::test_03_cast_tls_eke2_handshake_timeout),
        reg!("T2.F07.04: CEC Invalid Logical Address", 2, 7, boundary07_cast_cec_honesty::test_04_cec_invalid_logical_address),
        reg!("T2.F07.05: Cast Media Session Invalidation", 2, 7, boundary07_cast_cec_honesty::test_05_cast_media_session_id_invalidation),

        // Feature 8 Boundary
        reg!("T2.F08.01: Backpressure Drop-Oldest Policy", 2, 8, boundary08_observation_push_stream::test_01_backpressure_drop_oldest_policy),
        reg!("T2.F08.02: Lagged Subscriber Recovers", 2, 8, boundary08_observation_push_stream::test_02_lagged_subscriber_recovers),
        reg!("T2.F08.03: Zero Subscribers No Leak", 2, 8, boundary08_observation_push_stream::test_03_zero_subscribers_no_leak),
        reg!("T2.F08.04: Slow Consumer Does Not Block Fast", 2, 8, boundary08_observation_push_stream::test_04_slow_consumer_does_not_block_fast_consumer),
        reg!("T2.F08.05: Channel Close on Disconnect", 2, 8, boundary08_observation_push_stream::test_05_channel_close_on_driver_disconnect),

        // Feature 9 Boundary
        reg!("T2.F09.01: Orphaned Capability Causes Failure", 2, 9, boundary09_capability_conformance::test_01_orphaned_capability_causes_test_failure),
        reg!("T2.F09.02: Empty CapabilitySet Handling", 2, 9, boundary09_capability_conformance::test_02_empty_capability_set_handling),
        reg!("T2.F09.03: Conflicting Capability Resolution", 2, 9, boundary09_capability_conformance::test_03_conflicting_capability_resolution),
        reg!("T2.F09.04: Conformance Timeout Detection", 2, 9, boundary09_capability_conformance::test_04_conformance_timeout_detection),
        reg!("T2.F09.05: Dynamic Capability Demotion", 2, 9, boundary09_capability_conformance::test_05_dynamic_capability_demotion),

        // Feature 10 Boundary
        reg!("T2.F10.01: SemVer Dependency Alignment", 2, 10, boundary10_version_cadences::test_01_semver_incompatible_dependency_audit),
        reg!("T2.F10.02: Crate Publish Flags Enforced", 2, 10, boundary10_version_cadences::test_02_crate_publish_flags),
        reg!("T2.F10.03: Docs Versioning Table Completeness", 2, 10, boundary10_version_cadences::test_03_docs_versioning_table_completeness),
        reg!("T2.F10.04: Workspace Resolver Version 2", 2, 10, boundary10_version_cadences::test_04_workspace_resolver_version),
        reg!("T2.F10.05: MSRV 1.89 Consistent", 2, 10, boundary10_version_cadences::test_05_rust_version_msrv_consistent),

        // Feature 11 Boundary
        reg!("T2.F11.01: HIL Unconfigured Env Skips Gracefully", 2, 11, boundary11_hil_verification_matrix::test_01_hil_unconfigured_env_skips_gracefully),
        reg!("T2.F11.02: HIL Mock Device Justification", 2, 11, boundary11_hil_verification_matrix::test_02_hil_mock_device_explicit_justification),
        reg!("T2.F11.03: HIL Offline IP Error Handling", 2, 11, boundary11_hil_verification_matrix::test_03_hil_invalid_ip_returns_offline),
        reg!("T2.F11.04: Firmware Version Parsing Safety", 2, 11, boundary11_hil_verification_matrix::test_04_hil_firmware_version_parse_safety),
        reg!("T2.F11.05: Concurrent Device Lock Serialization", 2, 11, boundary11_hil_verification_matrix::test_05_hil_concurrent_device_lock),

        // Feature 12 Boundary
        reg!("T2.F12.01: Synthetic Frame Call Returns Error", 2, 12, boundary12_vision_honesty_fix::test_01_calling_synthetic_returns_unimplemented_error),
        reg!("T2.F12.02: Zero Sized Video Buffer Rejection", 2, 12, boundary12_vision_honesty_fix::test_02_zero_sized_video_buffer_rejected),
        reg!("T2.F12.03: Vision Dependency Isolated From Core", 2, 12, boundary12_vision_honesty_fix::test_03_vision_dependency_isolated_from_core),
        reg!("T2.F12.04: Unsupported Codec Typed Error", 2, 12, boundary12_vision_honesty_fix::test_04_unsupported_codec_error),
        reg!("T2.F12.05: Facade Without Vision Stability", 2, 12, boundary12_vision_honesty_fix::test_05_facade_without_vision_compiles),

        // ==========================================
        // TIER 3: Cross-Feature Combinations (12 tests)
        // ==========================================
        reg!("T3.01: Driver Registry + Discovery & Power", 3, 0, cross_feature_tests::test_01_pairwise_driver_registry_and_discovery_power),
        reg!("T3.02: Driver Registry + Capability Registry", 3, 0, cross_feature_tests::test_02_pairwise_driver_registry_and_capability_registry),
        reg!("T3.03: Tizen/webOS + Observation Stream", 3, 0, cross_feature_tests::test_03_pairwise_tizen_webos_and_observation_stream),
        reg!("T3.04: MDC Driver + Power + Conformance", 3, 0, cross_feature_tests::test_04_pairwise_mdc_driver_power_and_conformance),
        reg!("T3.05: Cast & CEC + Observation Stream", 3, 0, cross_feature_tests::test_05_pairwise_cast_cec_and_observation_stream),
        reg!("T3.06: Feature Matrix + Driver Registry", 3, 0, cross_feature_tests::test_06_pairwise_feature_matrix_and_driver_registry),
        reg!("T3.07: Core no_std + CapabilitySet", 3, 0, cross_feature_tests::test_07_pairwise_core_nostd_and_capability_set),
        reg!("T3.08: Deprecated Shims + Consolidated Drivers", 3, 0, cross_feature_tests::test_08_pairwise_deprecated_shims_and_consolidated_drivers),
        reg!("T3.09: Version Cadences + Feature Matrix", 3, 0, cross_feature_tests::test_09_pairwise_version_cadences_and_feature_matrix),
        reg!("T3.10: HIL Matrix + Driver Capabilities", 3, 0, cross_feature_tests::test_10_pairwise_hil_matrix_and_driver_capabilities),
        reg!("T3.11: Vision Honesty + Observation Stream", 3, 0, cross_feature_tests::test_11_pairwise_vision_honesty_and_observation_stream),
        reg!("T3.12: Conformance Harness + All 10 Drivers", 3, 0, cross_feature_tests::test_12_pairwise_conformance_harness_and_all_ten_drivers),

        // ==========================================
        // TIER 4: Real-World Scenarios (5 tests)
        // ==========================================
        reg!("T4.01: Commercial Video-Wall Sync (MDC + WoL)", 4, 0, real_world_scenarios::test_01_scenario_commercial_video_wall_sync),
        reg!("T4.02: Mixed Fleet Discovery & Control", 4, 0, real_world_scenarios::test_02_scenario_mixed_fleet_discovery_and_control),
        reg!("T4.03: Real-Time TV Observation Streaming", 4, 0, real_world_scenarios::test_03_scenario_realtime_observation_streaming),
        reg!("T4.04: Cast & CEC Unified Playback & Power", 4, 0, real_world_scenarios::test_04_scenario_cast_cec_unified_playback),
        reg!("T4.05: Legacy Crate Migration Compatibility", 4, 0, real_world_scenarios::test_05_scenario_legacy_crate_migration_compatibility),
    ]
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let filter_tier: Option<u8> = args.windows(2).find(|w| w[0] == "--tier").and_then(|w| w[1].parse().ok());
    let filter_feat: Option<u8> = args.windows(2).find(|w| w[0] == "--feature").and_then(|w| w[1].parse().ok());
    let json_mode = args.iter().any(|a| a == "--json");

    let all_tests = register_all_tests();
    let tests_to_run: Vec<&TestCase> = all_tests
        .iter()
        .filter(|t| filter_tier.map_or(true, |tier| t.tier == tier))
        .filter(|t| filter_feat.map_or(true, |feat| t.feature == feat))
        .collect();

    println!("========================================================");
    println!("  tv-rs 0.7.1 Opaque-Box E2E Test Suite (Runner)        ");
    println!("========================================================");
    println!("Registered Tests: {} | Filtered: {}", all_tests.len(), tests_to_run.len());

    let mut passed = 0;
    let mut failed = 0;
    let suite_start = Instant::now();

    for test in &tests_to_run {
        let t_start = Instant::now();
        let res = (test.func)().await;
        let elapsed = t_start.elapsed();

        match res {
            Ok(()) => {
                passed += 1;
                if !json_mode {
                    println!("  [PASS] {} ({:?})", test.name, elapsed);
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("  [FAIL] {} ({:?}): {}", test.name, elapsed, e);
            }
        }
    }

    let total_elapsed = suite_start.elapsed();
    println!("--------------------------------------------------------");
    println!("Results: {} Passed, {} Failed in {:?}", passed, failed, total_elapsed);
    println!("========================================================");

    if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
