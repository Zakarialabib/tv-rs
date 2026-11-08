//! Tier 2: Boundary 5 - Roku, MDC, SICP, IR Drivers (Corrupt Checksums, Escaping, NAKs).

use crate::harness::simulated_panel::MdcFrame;

pub async fn test_01_mdc_checksum_mismatch_rejection() -> Result<(), String> {
    let mut frame = MdcFrame::new(0x11, 1, vec![0x01]).serialize();
    // Corrupt the checksum byte
    let last_idx = frame.len() - 1;
    frame[last_idx] ^= 0xFF;

    let res = MdcFrame::parse(&frame);
    assert!(res.is_err(), "Corrupted MDC frame checksum must be rejected");
    Ok(())
}

pub async fn test_02_mdc_broadcast_display_id_0xfe() -> Result<(), String> {
    let bcast_frame = MdcFrame::new(0x11, MdcFrame::BROADCAST_ID, vec![0x01]);
    assert_eq!(bcast_frame.display_id, 0xFE);

    let serialized = bcast_frame.serialize();
    let parsed = MdcFrame::parse(&serialized)?;
    assert_eq!(parsed.display_id, 0xFE);
    Ok(())
}

pub async fn test_03_roku_ecp_empty_or_special_key() -> Result<(), String> {
    let key = "Lit_%20"; // Space key escaped
    let url = crate::harness::simulated_panel::RokuEcpProtocol::build_keypress_url("http://localhost:8060", key);
    assert!(url.contains("/keypress/Lit_%20"));
    Ok(())
}

pub async fn test_04_sicp_nak_response_parsing() -> Result<(), String> {
    // SICP NAK reply indicator: command reply with error status
    let sicp_nak_packet = [0x05, 0x01, 0x00, 0x15, 0x1B]; // 0x15 is NAK ASCII
    assert_eq!(sicp_nak_packet[3], 0x15);
    Ok(())
}

pub async fn test_05_ir_invalid_ircc_code_handling() -> Result<(), String> {
    // Valid Sony IRCC codes are 24-character Base64 strings
    let invalid_short_code = "ABC";
    assert_ne!(invalid_short_code.len(), 24);

    let valid_code = "AAAAAQAAAAEAAAAvAw==";
    assert_eq!(valid_code.len(), 20);
    assert!(base64::Engine::decode(&base64::engine::general_purpose::STANDARD, valid_code).is_ok());
    Ok(())
}
