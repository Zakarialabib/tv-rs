use std::collections::HashMap;

pub struct MdcFrame {
    pub header: u8,
    pub cmd: u8,
    pub display_id: u8,
    pub length: u8,
    pub data: Vec<u8>,
    pub checksum: u8,
}

impl MdcFrame {
    pub const HEADER: u8 = 0xAA;
    pub const BROADCAST_ID: u8 = 0xFE;

    pub fn new(cmd: u8, display_id: u8, data: Vec<u8>) -> Self {
        let length = data.len() as u8;
        let mut sum: u32 = cmd as u32 + display_id as u32 + length as u32;
        for b in &data {
            sum = sum.wrapping_add(*b as u32);
        }
        let checksum = (sum & 0xFF) as u8;

        Self {
            header: Self::HEADER,
            cmd,
            display_id,
            length,
            data,
            checksum,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(5 + self.data.len());
        buf.push(self.header);
        buf.push(self.cmd);
        buf.push(self.display_id);
        buf.push(self.length);
        buf.extend_from_slice(&self.data);
        buf.push(self.checksum);
        buf
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 5 {
            return Err("MDC frame too short".to_string());
        }
        if bytes[0] != Self::HEADER {
            return Err(format!("Invalid MDC header byte: expected 0xAA, got 0x{:02X}", bytes[0]));
        }
        let cmd = bytes[1];
        let display_id = bytes[2];
        let length = bytes[3] as usize;

        if bytes.len() < 5 + length {
            return Err("MDC data buffer truncated".to_string());
        }

        let data = bytes[4..4 + length].to_vec();
        let expected_checksum = bytes[4 + length];

        let mut sum: u32 = cmd as u32 + display_id as u32 + length as u32;
        for b in &data {
            sum = sum.wrapping_add(*b as u32);
        }
        let computed_checksum = (sum & 0xFF) as u8;

        if computed_checksum != expected_checksum {
            return Err(format!(
                "MDC checksum mismatch: computed 0x{:02X}, received 0x{:02X}",
                computed_checksum, expected_checksum
            ));
        }

        Ok(Self {
            header: bytes[0],
            cmd,
            display_id,
            length: length as u8,
            data,
            checksum: expected_checksum,
        })
    }
}

pub struct WolPacket;

impl WolPacket {
    pub fn build_magic_packet(mac_str: &str) -> Result<Vec<u8>, String> {
        let mac_bytes = Self::parse_mac(mac_str)?;
        let mut packet = Vec::with_capacity(102);
        // 6 bytes of 0xFF
        packet.extend_from_slice(&[0xFF; 6]);
        // 16 repetitions of 6-byte MAC
        for _ in 0..16 {
            packet.extend_from_slice(&mac_bytes);
        }
        Ok(packet)
    }

    pub fn verify_magic_packet(packet: &[u8], expected_mac_str: &str) -> Result<bool, String> {
        if packet.len() != 102 {
            return Ok(false);
        }
        if &packet[0..6] != &[0xFF; 6] {
            return Ok(false);
        }
        let expected_mac = Self::parse_mac(expected_mac_str)?;
        for chunk_idx in 0..16 {
            let offset = 6 + (chunk_idx * 6);
            if &packet[offset..offset + 6] != expected_mac.as_slice() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn parse_mac(mac_str: &str) -> Result<[u8; 6], String> {
        let clean = mac_str.replace([':', '-'], "");
        if clean.len() != 12 {
            return Err(format!("Invalid MAC address length: {}", mac_str));
        }
        let mut mac = [0u8; 6];
        for i in 0..6 {
            mac[i] = u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16)
                .map_err(|e| format!("Invalid MAC hex digit: {}", e))?;
        }
        Ok(mac)
    }
}

pub struct SsdpProtocol;

impl SsdpProtocol {
    pub const M_SEARCH_TARGET: &'static str = "239.255.255.250:1900";

    pub fn build_msearch(search_target: &str) -> String {
        format!(
            "M-SEARCH * HTTP/1.1\r\n\
            HOST: 239.255.255.250:1900\r\n\
            MAN: \"ssdp:discover\"\r\n\
            MX: 3\r\n\
            ST: {}\r\n\
            \r\n",
            search_target
        )
    }

    pub fn parse_upnp_xml(xml: &str) -> Result<HashMap<String, String>, String> {
        let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
        let mut map = HashMap::new();

        if let Some(device) = doc.descendants().find(|n| n.has_tag_name("device")) {
            for child in device.children() {
                if child.is_element() {
                    let tag = child.tag_name().name().to_string();
                    if let Some(text) = child.text() {
                        map.insert(tag, text.to_string());
                    }
                }
            }
        }

        Ok(map)
    }
}

pub struct RokuEcpProtocol;

impl RokuEcpProtocol {
    pub fn build_keypress_url(base_url: &str, key: &str) -> String {
        format!("{}/keypress/{}", base_url.trim_end_matches('/'), key)
    }

    pub fn build_launch_url(base_url: &str, app_id: &str) -> String {
        format!("{}/launch/{}", base_url.trim_end_matches('/'), app_id)
    }

    pub fn build_query_apps_url(base_url: &str) -> String {
        format!("{}/query/apps", base_url.trim_end_matches('/'))
    }
}

pub struct WebOsSsapProtocol;

impl WebOsSsapProtocol {
    pub fn build_register_payload(client_key: Option<&str>) -> serde_json::Value {
        let mut manifest = serde_json::json!({
            "manifest": {
                "manifestVersion": 1,
                "appVersion": "1.0.0",
                "signed": {
                    "created": "20261107",
                    "appId": "com.tv-rs.remote",
                    "vendorId": "tv-rs",
                    "localizedAppNames": {
                        "": "tv-rs Remote"
                    },
                    "permissions": [
                        "CONTROL_AUDIO",
                        "CONTROL_POWER",
                        "READ_RUNNING_APPS"
                    ]
                }
            }
        });

        if let Some(key) = client_key {
            manifest["client-key"] = serde_json::Value::String(key.to_string());
        }

        serde_json::json!({
            "type": "register",
            "id": "register_0",
            "payload": manifest
        })
    }
}

pub struct TizenWsProtocol;

impl TizenWsProtocol {
    pub fn build_handshake_url(ip: &str, port: u16, app_name: &str, token: Option<&str>) -> String {
        let b64_name = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, app_name);
        let mut url = format!("wss://{}:{}/api/v2/channels/samsung.remote.control?name={}", ip, port, b64_name);
        if let Some(tok) = token {
            url.push_str(&format!("&token={}", tok));
        }
        url
    }
}

pub struct SonyIrccProtocol;

impl SonyIrccProtocol {
    pub fn build_soap_envelope(ircc_code: &str) -> String {
        format!(
            "<?xml version=\"1.0\"?>\r\n\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\r\n\
              <s:Body>\r\n\
                <u:X_SendIRCC xmlns:u=\"urn:schemas-sony-com:service:IRCC:1\">\r\n\
                  <IRCCCode>{}</IRCCCode>\r\n\
                </u:X_SendIRCC>\r\n\
              </s:Body>\r\n\
            </s:Envelope>",
            ircc_code
        )
    }
}
