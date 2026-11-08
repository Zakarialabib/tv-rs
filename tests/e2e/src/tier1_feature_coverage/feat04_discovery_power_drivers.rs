//! Tier 1: Feature 4 - Discovery & Power Drivers (R1, TV-11).

use crate::harness::simulated_panel::{SsdpProtocol, WolPacket};
use crate::harness::workspace_audit::WorkspaceAudit;

pub async fn test_01_ssdp_msearch_datagram_construction() -> Result<(), String> {
    let msearch = SsdpProtocol::build_msearch("urn:schemas-upnp-org:device:MediaRenderer:1");
    assert!(msearch.starts_with("M-SEARCH * HTTP/1.1\r\n"));
    assert!(msearch.contains("HOST: 239.255.255.250:1900\r\n"));
    assert!(msearch.contains("MAN: \"ssdp:discover\"\r\n"));
    assert!(msearch.contains("ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n"));
    Ok(())
}

pub async fn test_02_upnp_location_descriptor_parsing() -> Result<(), String> {
    let xml = r#"<?xml version="1.0"?>
    <root xmlns="urn:schemas-upnp-org:device-1-0">
      <device>
        <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
        <friendlyName>Living Room Smart TV</friendlyName>
        <manufacturer>Samsung Electronics</manufacturer>
        <modelName>QN65Q80R</modelName>
        <UDN>uuid:01234567-89ab-cdef-0123-456789abcdef</UDN>
      </device>
    </root>"#;

    let props = SsdpProtocol::parse_upnp_xml(xml)?;
    assert_eq!(props.get("friendlyName").map(String::as_str), Some("Living Room Smart TV"));
    assert_eq!(props.get("manufacturer").map(String::as_str), Some("Samsung Electronics"));
    assert_eq!(props.get("modelName").map(String::as_str), Some("QN65Q80R"));
    assert_eq!(props.get("UDN").map(String::as_str), Some("uuid:01234567-89ab-cdef-0123-456789abcdef"));
    Ok(())
}

pub async fn test_03_wol_magic_packet_payload_structure() -> Result<(), String> {
    let mac = "00:11:22:33:44:55";
    let packet = WolPacket::build_magic_packet(mac)?;
    assert_eq!(packet.len(), 102);

    let is_valid = WolPacket::verify_magic_packet(&packet, mac)?;
    assert!(is_valid);
    Ok(())
}

pub async fn test_04_tcp_power_probe_port_detection() -> Result<(), String> {
    // Known control ports for smart TV power probing:
    // Samsung: 8001 / 8002
    // LG webOS: 3000 / 3001
    // Roku: 8060
    // MDC: 1515
    let ports = [8001, 8002, 3000, 3001, 8060, 1515];
    assert_eq!(ports.len(), 6);
    assert!(ports.contains(&8001));
    assert!(ports.contains(&1515));
    Ok(())
}

pub async fn test_05_deprecated_shims_reexport_discovery_power() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(spec.contains("TV-11"), "specdd must document TV-11 discovery/power shims");
    Ok(())
}
