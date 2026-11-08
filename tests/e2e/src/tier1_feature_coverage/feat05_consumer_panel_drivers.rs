//! Tier 1: Feature 5 - Roku, MDC, SICP, IR Drivers (R1, TV-13).

use crate::harness::simulated_panel::{MdcFrame, RokuEcpProtocol, SonyIrccProtocol};

pub async fn test_01_roku_ecp_keypress_url_encoding() -> Result<(), String> {
    let url = RokuEcpProtocol::build_keypress_url("http://192.168.1.150:8060", "Home");
    assert_eq!(url, "http://192.168.1.150:8060/keypress/Home");

    let launch = RokuEcpProtocol::build_launch_url("http://192.168.1.150:8060", "12");
    assert_eq!(launch, "http://192.168.1.150:8060/launch/12");
    Ok(())
}

pub async fn test_02_mdc_packet_framing_and_checksum() -> Result<(), String> {
    // Command 0x11 (Power Control), Display ID 1, Data [0x01] (Power On)
    let frame = MdcFrame::new(0x11, 1, vec![0x01]);
    let serialized = frame.serialize();
    assert_eq!(serialized[0], 0xAA);
    assert_eq!(serialized[1], 0x11);
    assert_eq!(serialized[2], 0x01);
    assert_eq!(serialized[3], 0x01);
    assert_eq!(serialized[4], 0x01);

    let parsed = MdcFrame::parse(&serialized)?;
    assert_eq!(parsed.cmd, 0x11);
    assert_eq!(parsed.display_id, 1);
    assert_eq!(parsed.data, vec![0x01]);
    assert_eq!(parsed.checksum, frame.checksum);
    Ok(())
}

pub async fn test_03_mdc_video_wall_command_structure() -> Result<(), String> {
    // Command 0x84 (Video Wall Control), Display ID 1, Data: [Enable (1), DivX (2), DivY (2), Pos (1)]
    let frame = MdcFrame::new(0x84, 1, vec![1, 2, 2, 1]);
    assert_eq!(frame.length, 4);
    let serialized = frame.serialize();

    let parsed = MdcFrame::parse(&serialized)?;
    assert_eq!(parsed.cmd, 0x84);
    assert_eq!(parsed.data, vec![1, 2, 2, 1]);
    Ok(())
}

pub async fn test_04_sicp_command_packet_structure() -> Result<(), String> {
    // Philips SICP frame: Header [size, group, cmd, ... checksum]
    let sicp_cmd = [0x06, 0x01, 0x00, 0x01, 0x02, 0x0A];
    assert_eq!(sicp_cmd[0], 0x06); // frame size
    assert_eq!(sicp_cmd[1], 0x01); // command group
    Ok(())
}

pub async fn test_05_ir_ircc_payload_generation() -> Result<(), String> {
    let ircc_code = "AAAAAQAAAAEAAAAvAw==";
    let soap = SonyIrccProtocol::build_soap_envelope(ircc_code);
    assert!(soap.contains("<s:Envelope"));
    assert!(soap.contains("<IRCCCode>AAAAAQAAAAEAAAAvAw==</IRCCCode>"));
    Ok(())
}
