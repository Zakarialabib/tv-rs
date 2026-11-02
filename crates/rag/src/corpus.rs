//! Static vendor quirks and protocol specifications corpus (Tier A Knowledge Base).
//!
//! Stores pre-chunked documentation covering Samsung MDC, Philips SICP, LG webOS SSAP,
//! Tizen WebSocket, Roku ECP, and EDID quirks. Chunks are metadata-tagged for exact
//! pre-filtering and token-level lexical search.

use serde::{Deserialize, Serialize};
use tv_rs_core::quirks::Quirks;

/// A chunk of documentation representing a protocol spec, vendor quirk, or hardware defect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolDocChunk {
    /// Unique identifier for this chunk (e.g. `samsung_mdc_0x84_videowall`).
    pub id: String,
    /// Vendor name in lowercase (e.g. `samsung`, `philips`, `lg`, `roku`).
    pub vendor: String,
    /// Specific protocol (e.g. `mdc`, `sicp`, `ssap`, `tizen_ws`, `ecp`, `edid`).
    pub protocol: String,
    /// Optional model prefix restriction (e.g. `QB`, `QN90`).
    pub model_prefix: Option<String>,
    /// Issue or feature identifier (e.g. `video_wall_0x84`, `ws_8001_disabled`).
    pub issue_id: String,
    /// Relevant capability string if applicable (e.g. `video_wall`, `power`, `input`).
    pub capability: Option<String>,
    /// Short descriptive title.
    pub title: String,
    /// Detailed text content / explanation of the quirk, opcode, or protocol fact.
    pub content: String,
    /// Exact hex byte sequences or command signatures (e.g. `["0x84", "0x56", "0x11"]`).
    pub hex_signatures: Vec<String>,
    /// Searchable tags and tokens.
    pub tags: Vec<String>,
}

impl ProtocolDocChunk {
    /// Checks whether this chunk matches metadata filters.
    #[must_use]
    pub fn matches_filter(
        &self,
        vendor: Option<&str>,
        protocol: Option<&str>,
        model_prefix: Option<&str>,
        capability: Option<&str>,
    ) -> bool {
        if let Some(v) = vendor {
            if !self.vendor.eq_ignore_ascii_case(v) {
                return false;
            }
        }
        if let Some(p) = protocol {
            if !self.protocol.eq_ignore_ascii_case(p) {
                return false;
            }
        }
        if let Some(m) = model_prefix {
            if let Some(prefix) = &self.model_prefix {
                if !m.starts_with(prefix) && !prefix.starts_with(m) {
                    return false;
                }
            }
        }
        if let Some(cap) = capability {
            if let Some(c) = &self.capability {
                if !c.eq_ignore_ascii_case(cap) {
                    return false;
                }
            }
        }
        true
    }
}

/// The corpus containing static protocol specs, vendor quirks, and hardware facts.
#[derive(Debug, Clone, Default)]
pub struct Corpus {
    chunks: Vec<ProtocolDocChunk>,
}

impl Corpus {
    /// Creates an empty corpus.
    #[must_use]
    pub fn new() -> Self {
        Self { chunks: Vec::new() }
    }

    /// Adds a chunk to the corpus.
    pub fn add_chunk(&mut self, chunk: ProtocolDocChunk) {
        self.chunks.push(chunk);
    }

    /// Returns a slice of all chunks in the corpus.
    #[must_use]
    pub fn chunks(&self) -> &[ProtocolDocChunk] {
        &self.chunks
    }

    /// Ingests all quirks from `tv_rs_core::quirks::Quirks`.
    pub fn ingest_quirks(&mut self, quirks: &Quirks) {
        for quirk in quirks.entries() {
            let chunk = ProtocolDocChunk {
                id: format!("{}_{}", quirk.vendor, quirk.issue),
                vendor: quirk.vendor.clone(),
                protocol: match quirk.vendor.as_str() {
                    "samsung" => "tizen_ws".to_string(),
                    "lg" => "ssap".to_string(),
                    "roku" => "ecp".to_string(),
                    other => other.to_string(),
                },
                model_prefix: quirk.model_prefix.clone(),
                issue_id: quirk.issue.clone(),
                capability: None,
                title: format!("{} Quirk: {}", quirk.vendor, quirk.issue),
                content: format!(
                    "Defect issue '{}' for vendor '{}'. Action to take: '{}'. Note: {}",
                    quirk.issue,
                    quirk.vendor,
                    quirk.action,
                    quirk.note.as_deref().unwrap_or("None")
                ),
                hex_signatures: Vec::new(),
                tags: vec![
                    quirk.vendor.clone(),
                    quirk.issue.clone(),
                    quirk.action.clone(),
                ],
            };
            self.add_chunk(chunk);
        }
    }

    /// Populates canonical built-in protocol facts for Samsung MDC, Philips SICP,
    /// webOS SSAP, Tizen WebSocket, and EDID specifications.
    pub fn ingest_canonical_specs(&mut self) {
        // 1. Samsung MDC Video Wall (0x84)
        self.add_chunk(ProtocolDocChunk {
            id: "samsung_mdc_0x84_videowall".to_string(),
            vendor: "samsung".to_string(),
            protocol: "mdc".to_string(),
            model_prefix: None,
            issue_id: "videowall_matrix_mode".to_string(),
            capability: Some("video_wall".to_string()),
            title: "Samsung MDC 0x84 Video Wall Mode Specification".to_string(),
            content: "Samsung Multiple Display Control (MDC) 0x84 command controls Video Wall mode. \
                      Payload format: [CMD: 0x84, ID: display_id, LEN: 0x02, DATA: [mode, val]]. \
                      Mode 0x01 enables video wall division; second byte specifies matrix layout (e.g. 2x2, 3x3) \
                      and screen tile positioning. TCP port 1515 or RS-232 9600 baud. Error responses return \
                      NACK 0xFF with error code bytes.".to_string(),
            hex_signatures: vec!["0x84".to_string(), "0x1515".to_string(), "0xFF".to_string()],
            tags: vec![
                "samsung".to_string(),
                "mdc".to_string(),
                "videowall".to_string(),
                "0x84".to_string(),
                "1515".to_string(),
            ],
        });

        // 2. Samsung MDC Power (0x11)
        self.add_chunk(ProtocolDocChunk {
            id: "samsung_mdc_0x11_power".to_string(),
            vendor: "samsung".to_string(),
            protocol: "mdc".to_string(),
            model_prefix: None,
            issue_id: "mdc_power_control".to_string(),
            capability: Some("power".to_string()),
            title: "Samsung MDC 0x11 Power Control Specification".to_string(),
            content: "Samsung MDC Power Control uses command code 0x11. Frame structure: Header 0xAA, \
                      Command 0x11, Display ID, Length 0x01, Data [0x01 for ON, 0x00 for OFF], Checksum. \
                      When waking from deep standby over RS-232, a 4s delay is required after transmission.".to_string(),
            hex_signatures: vec!["0x11".to_string(), "0xAA".to_string()],
            tags: vec!["samsung".to_string(), "mdc".to_string(), "power".to_string(), "0x11".to_string(), "rs232".to_string()],
        });

        // 3. Philips SICP Video Wall (0x56)
        self.add_chunk(ProtocolDocChunk {
            id: "philips_sicp_0x56_videowall".to_string(),
            vendor: "philips".to_string(),
            protocol: "sicp".to_string(),
            model_prefix: Some("BDL".to_string()),
            issue_id: "sicp_videowall_tile".to_string(),
            capability: Some("video_wall".to_string()),
            title: "Philips SICP 0x56 Video Wall Tile Positioning".to_string(),
            content: "Philips Signage Interactive Control Protocol (SICP) command 0x56 configures video wall \
                      tile positioning and matrix dimensions over TCP port 5000 or RS-232. \
                      Payload format: [Length: 0x08, MsgType: 0x01, Group: 0x00, Addr: ID, Cmd: 0x56, Data: [h_tiles, v_tiles, h_pos, v_pos]]. \
                      Acknowledgement is returned with MsgType 0x02. Supports displays like BDL series.".to_string(),
            hex_signatures: vec!["0x56".to_string(), "5000".to_string()],
            tags: vec![
                "philips".to_string(),
                "sicp".to_string(),
                "videowall".to_string(),
                "0x56".to_string(),
                "5000".to_string(),
                "tile".to_string(),
            ],
        });

        // 4. LG webOS SSAP Ports and TLS
        self.add_chunk(ProtocolDocChunk {
            id: "lg_webos_ssap_ports".to_string(),
            vendor: "lg".to_string(),
            protocol: "ssap".to_string(),
            model_prefix: None,
            issue_id: "ssap_port_handshake".to_string(),
            capability: Some("transport".to_string()),
            title: "LG webOS SSAP WebSocket Port & Pairing Protocol".to_string(),
            content: "LG webOS uses the SSAP JSON-RPC WebSocket protocol. Firmware versions prior to webOS 23 \
                      listened on plaintext ws://:3000. Starting in webOS 23/24, port 3000 is removed and clients \
                      must use wss://:3001 with self-signed TLS pinning. First-time registration issues a pairing \
                      prompt on screen; consumer webOS returns no MAC query API via SSAP.".to_string(),
            hex_signatures: vec!["3000".to_string(), "3001".to_string()],
            tags: vec!["lg".to_string(), "webos".to_string(), "ssap".to_string(), "3000".to_string(), "3001".to_string(), "tls".to_string()],
        });

        // 5. Tizen WebSocket Ports & Pairing
        self.add_chunk(ProtocolDocChunk {
            id: "samsung_tizen_ws_pairing".to_string(),
            vendor: "samsung".to_string(),
            protocol: "tizen_ws".to_string(),
            model_prefix: None,
            issue_id: "tizen_ws_8001_8002".to_string(),
            capability: Some("transport".to_string()),
            title: "Samsung Tizen Remote Channel Ports & Token Auth".to_string(),
            content: "Samsung Tizen TVs listen on ws://:8001 (legacy/plaintext) and wss://:8002 (TLS token v2). \
                      2020+ Neo QLED firmwares (T-KTM) drop port 8001 entirely. Querying port 8001 results in \
                      connection timeout. Installed app listing via ed.installedApp.get is supported only on Tizen <= 4; \
                      Tizen 5+ silently drops the listing request.".to_string(),
            hex_signatures: vec!["8001".to_string(), "8002".to_string()],
            tags: vec!["samsung".to_string(), "tizen".to_string(), "8001".to_string(), "8002".to_string(), "token".to_string()],
        });

        // 6. EDID Vendor Quirks
        self.add_chunk(ProtocolDocChunk {
            id: "edid_vendor_quirks".to_string(),
            vendor: "generic".to_string(),
            protocol: "edid".to_string(),
            model_prefix: None,
            issue_id: "edid_pnp_identification".to_string(),
            capability: Some("probe".to_string()),
            title: "EDID PNP Manufacturer Codes & Commercial Monitor Likelihood".to_string(),
            content: "Local display probing reads 128-byte EDID structures. Three-letter PNP manufacturer codes: \
                      'SAM' -> Samsung, 'LGE' -> LG Electronics, 'DEL' -> Dell, 'PHL' -> Philips. \
                      Commercial LFDs can be distinguished from standard desktop monitors by physical dimensions (>32 inch), \
                      detailed timing descriptors (1080p/4K @ 60Hz), and RS-232/DDC-CI VCP code 0xD6 power capability.".to_string(),
            hex_signatures: vec!["0xD6".to_string()],
            tags: vec!["edid".to_string(), "probe".to_string(), "sam".to_string(), "lge".to_string(), "del".to_string(), "phl".to_string()],
        });
    }

    /// Loads the default bundled knowledge base including bundled quirks and canonical specs.
    ///
    /// # Errors
    ///
    /// Returns an error if bundled quirks fail to parse.
    pub fn default_knowledge_base() -> Result<Self, tv_rs_core::quirks::QuirkError> {
        let mut corpus = Self::new();
        let quirks = Quirks::bundled()?;
        corpus.ingest_quirks(&quirks);
        corpus.ingest_canonical_specs();
        Ok(corpus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_knowledge_base_loads_chunks() {
        let corpus = Corpus::default_knowledge_base().expect("parses default knowledge base");
        assert!(!corpus.chunks().is_empty());
        // Verify canonical specs exist
        let mdc = corpus
            .chunks()
            .iter()
            .find(|c| c.id == "samsung_mdc_0x84_videowall");
        assert!(mdc.is_some());
        assert_eq!(mdc.unwrap().protocol, "mdc");

        let sicp = corpus
            .chunks()
            .iter()
            .find(|c| c.id == "philips_sicp_0x56_videowall");
        assert!(sicp.is_some());
        assert_eq!(sicp.unwrap().protocol, "sicp");
    }

    #[test]
    fn filter_matching_works() {
        let chunk = ProtocolDocChunk {
            id: "test".to_string(),
            vendor: "philips".to_string(),
            protocol: "sicp".to_string(),
            model_prefix: Some("BDL".to_string()),
            issue_id: "test_issue".to_string(),
            capability: Some("video_wall".to_string()),
            title: "Test".to_string(),
            content: "Content".to_string(),
            hex_signatures: vec![],
            tags: vec![],
        };

        assert!(chunk.matches_filter(
            Some("philips"),
            Some("sicp"),
            Some("BDL5000"),
            Some("video_wall")
        ));
        assert!(!chunk.matches_filter(Some("samsung"), None, None, None));
        assert!(!chunk.matches_filter(None, Some("mdc"), None, None));
    }
}
