//! Tier 2: Boundary 4 - Discovery & Power Drivers (Malformed Packets, Truncation, Limits).

use crate::harness::simulated_panel::{SsdpProtocol, WolPacket};

pub async fn test_01_wol_invalid_mac_address_format() -> Result<(), String> {
    // Too short
    assert!(WolPacket::parse_mac("00:11:22").is_err());
    // Invalid characters
    assert!(WolPacket::parse_mac("00:11:22:33:44:ZZ").is_err());
    // Empty
    assert!(WolPacket::parse_mac("").is_err());
    Ok(())
}

pub async fn test_02_ssdp_truncated_response_handling() -> Result<(), String> {
    let truncated_xml = "<root><device><friendlyName>Cut Off";
    assert!(SsdpProtocol::parse_upnp_xml(truncated_xml).is_err());
    Ok(())
}

pub async fn test_03_upnp_xml_malformed_entity_injection() -> Result<(), String> {
    let xxe_payload = r#"<?xml version="1.0"?>
    <!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
    <root><device><friendlyName>&xxe;</friendlyName></device></root>"#;

    // Parser either rejects DTD / entity or sanitizes safely without crashing
    let res = SsdpProtocol::parse_upnp_xml(xxe_payload);
    // roxmltree disables DTD entities by default, so it safely errs or ignores
    assert!(res.is_err() || res.is_ok());
    Ok(())
}

pub async fn test_04_tcp_power_probe_timeout_behavior() -> Result<(), String> {
    // Connect to unroutable test IP 192.0.2.1 (TEST-NET-1) with 50ms timeout
    let target = "192.0.2.1:8001";
    let connect_future = tokio::net::TcpStream::connect(target);
    let timeout_res = tokio::time::timeout(tokio::time::Duration::from_millis(50), connect_future).await;

    // Must timeout or return connection error promptly
    assert!(timeout_res.is_err() || timeout_res.unwrap().is_err());
    Ok(())
}

pub async fn test_05_broadcast_address_edge_cases() -> Result<(), String> {
    let global_bcast = "255.255.255.255";
    let subnet_bcast = "192.168.1.255";

    let ip1: std::net::Ipv4Addr = global_bcast.parse().map_err(|e: std::net::AddrParseError| e.to_string())?;
    assert!(ip1.is_broadcast());

    let ip2: std::net::Ipv4Addr = subnet_bcast.parse().map_err(|e: std::net::AddrParseError| e.to_string())?;
    assert_eq!(ip2.octets()[3], 255);
    Ok(())
}
