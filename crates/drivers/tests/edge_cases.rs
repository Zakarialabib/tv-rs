//! Edge case tests for tv-rs-drivers registry, command acks, mock streams, and descriptors.

use tv_rs_core::command::{Command, CommandAck, CommandId};
use tv_rs_core::descriptor::DiscoveredTv;
use tv_rs_core::observe::{ObservationStream, TvEvent};
use tv_rs_core::state::ObservedState;
use tv_rs_core::types::TvVendor;
use tv_rs_drivers::registry::DriverRegistry;

#[test]
fn test_empty_registry_edge_cases() {
    let empty_reg = DriverRegistry::new();
    let drivers = empty_reg.into_drivers();
    // Empty registry must return an empty Vec
    assert!(drivers.is_empty());
    assert_eq!(drivers.len(), 0);

    // Iterating over empty registry should not panic
    let mut count = 0;
    for _ in &drivers {
        count += 1;
    }
    assert_eq!(count, 0);

    // Finding driver for any vendor returns None without panic
    let samsung_driver = drivers.iter().find(|d| d.vendor() == TvVendor::Samsung);
    assert!(samsung_driver.is_none());

    let unknown_driver = drivers.iter().find(|d| d.vendor() == TvVendor::Unknown);
    assert!(unknown_driver.is_none());
}

#[test]
fn test_command_ack_and_unrecognized_command() {
    // 1. Default ack without id or message
    let ack_default = CommandAck::new();
    assert!(ack_default.id.is_none());
    assert!(ack_default.message.is_none());

    // 2. Ack with id
    let cid = CommandId::new();
    let ack_with_id = CommandAck::with_id(cid);
    assert_eq!(ack_with_id.id, Some(cid));
    assert!(ack_with_id.message.is_none());

    // 3. Ack for unrecognized command with explicit message and no id
    let ack_unrecognized = CommandAck {
        id: None,
        message: Some("Unknown command: REBOOT_RECOVERY".to_string()),
    };
    assert!(ack_unrecognized.id.is_none());
    assert_eq!(
        ack_unrecognized.message.as_deref(),
        Some("Unknown command: REBOOT_RECOVERY")
    );

    // 4. Equality & Clone of CommandAck
    let cloned_ack = ack_unrecognized.clone();
    assert_eq!(cloned_ack, ack_unrecognized);

    // 5. Command enum variants construction
    let custom_cmd = Command::Custom {
        name: "RAW_HEX".to_string(),
        payload: "0xAA 0x11".to_string(),
    };
    match custom_cmd {
        Command::Custom { name, payload } => {
            assert_eq!(name, "RAW_HEX");
            assert_eq!(payload, "0xAA 0x11");
        }
        _ => panic!("expected Custom variant"),
    }
}

#[test]
fn test_observation_stream_and_events() {
    // ObservationStream construction
    let stream = ObservationStream::new();
    assert_eq!(format!("{stream:?}"), "ObservationStream { _private: () }");

    // TvEvent variants
    let conn_event = TvEvent::ConnectionChanged(true);
    let disc_event = TvEvent::ConnectionChanged(false);
    assert_ne!(conn_event, disc_event);
    assert_eq!(conn_event.clone(), conn_event);

    let state = ObservedState::default();
    let state_event = TvEvent::StateChanged(state.clone());
    let state_event2 = TvEvent::StateChanged(state);
    assert_eq!(state_event, state_event2);
    assert_ne!(state_event, conn_event);
}

#[test]
fn test_discovered_tv_label_fallback_edge_cases() {
    // 1. Only IP provided (probed default)
    let tv1 = DiscoveredTv::probed(TvVendor::Lg, "192.168.1.99");
    assert_eq!(tv1.label(), "192.168.1.99");

    // 2. Model name provided, friendly name is None
    let mut tv2 = tv1.clone();
    tv2.model_name = Some("OLED65C1".to_string());
    assert_eq!(tv2.label(), "OLED65C1");

    // 3. Friendly name provided (takes precedence over model name)
    let mut tv3 = tv2.clone();
    tv3.friendly_name = Some("Family Room TV".to_string());
    assert_eq!(tv3.label(), "Family Room TV");

    // 4. Empty string IP edge case
    let tv_empty_ip = DiscoveredTv::probed(TvVendor::Unknown, "");
    assert_eq!(tv_empty_ip.label(), "");

    // 5. IPv6 address edge case
    let tv_ipv6 = DiscoveredTv::probed(TvVendor::Samsung, "[fe80::1ff:fe23:4567:890a]");
    assert_eq!(tv_ipv6.label(), "[fe80::1ff:fe23:4567:890a]");
}

#[test]
fn test_all_twelve_tv_vendors() {
    let all_vendors = [
        TvVendor::Samsung,
        TvVendor::Lg,
        TvVendor::Roku,
        TvVendor::Philips,
        TvVendor::GoogleCast,
        TvVendor::Cec,
        TvVendor::Mdc,
        TvVendor::Sicp,
        TvVendor::Ir,
        TvVendor::Tizen,
        TvVendor::WebOs,
        TvVendor::Unknown,
    ];

    assert_eq!(all_vendors.len(), 12);
    for vendor in &all_vendors {
        // String label is not empty
        assert!(!vendor.as_str().is_empty());
        // Display matches as_str
        assert_eq!(vendor.to_string(), vendor.as_str());
    }
}
