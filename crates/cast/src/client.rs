//! Cast client implementation mirroring other vendor adapters.

use crate::error::Result;
use crate::payload::{
    ConnectionRequest, MediaInfo, MediaRequest, ReceiverRequest, ReceiverStatus, Volume,
};
use crate::proto::cast_channel::{cast_message, CastMessage};
use crate::transport::CastTransport;

/// Default receiver ID used for launching standard apps.
pub const DEFAULT_RECEIVER_ID: &str = "receiver-0";
/// Sender ID used by this client.
pub const SENDER_ID: &str = "sender-0";
/// CC1AD845 is the Default Media Receiver app ID.
pub const DEFAULT_MEDIA_RECEIVER_APP_ID: &str = "CC1AD845";

/// Google Cast client.
pub struct CastClient {
    transport: CastTransport,
    request_id: i32,
}

impl CastClient {
    /// Connects to the Cast device at the given IP address.
    pub async fn connect(ip: &str) -> Result<Self> {
        let mut transport = CastTransport::connect(ip).await?;

        // Send connection request
        let connect_req = ConnectionRequest::Connect;
        let connect_msg = CastMessage {
            protocol_version: cast_message::ProtocolVersion::Castv210 as i32,
            source_id: SENDER_ID.to_string(),
            destination_id: DEFAULT_RECEIVER_ID.to_string(),
            namespace: "urn:x-cast:com.google.cast.tp.connection".to_string(),
            payload_type: cast_message::PayloadType::String as i32,
            payload_utf8: Some(serde_json::to_string(&connect_req)?),
            payload_binary: None,
        };
        transport.write_message(&connect_msg).await?;

        Ok(Self {
            transport,
            request_id: 1,
        })
    }

    fn next_request_id(&mut self) -> i32 {
        let id = self.request_id;
        self.request_id += 1;
        id
    }

    /// Fetches the receiver status (appId, displayName, volume, etc).
    pub async fn status(&mut self) -> Result<ReceiverStatus> {
        let req_id = self.next_request_id();
        let get_status = ReceiverRequest::GetStatus { request_id: req_id };

        let msg = CastMessage {
            protocol_version: cast_message::ProtocolVersion::Castv210 as i32,
            source_id: SENDER_ID.to_string(),
            destination_id: DEFAULT_RECEIVER_ID.to_string(),
            namespace: "urn:x-cast:com.google.cast.receiver".to_string(),
            payload_type: cast_message::PayloadType::String as i32,
            payload_utf8: Some(serde_json::to_string(&get_status)?),
            payload_binary: None,
        };
        self.transport.write_message(&msg).await?;

        // Read until RECEIVER_STATUS is found
        loop {
            let reply = self.transport.read_message().await?;
            if reply.namespace == "urn:x-cast:com.google.cast.receiver" {
                if let Some(payload_str) = reply.payload_utf8 {
                    let resp: crate::payload::ReceiverResponse =
                        serde_json::from_str(&payload_str)?;
                    match resp {
                        crate::payload::ReceiverResponse::ReceiverStatus { status, .. } => {
                            return Ok(status);
                        }
                        _ => continue,
                    }
                }
            }
        }
    }

    /// Launches an application by ID.
    pub async fn launch(&mut self, app_id: &str) -> Result<()> {
        let req_id = self.next_request_id();
        let launch_req = ReceiverRequest::Launch {
            request_id: req_id,
            app_id: app_id.to_string(),
        };

        let msg = CastMessage {
            protocol_version: cast_message::ProtocolVersion::Castv210 as i32,
            source_id: SENDER_ID.to_string(),
            destination_id: DEFAULT_RECEIVER_ID.to_string(),
            namespace: "urn:x-cast:com.google.cast.receiver".to_string(),
            payload_type: cast_message::PayloadType::String as i32,
            payload_utf8: Some(serde_json::to_string(&launch_req)?),
            payload_binary: None,
        };
        self.transport.write_message(&msg).await?;
        Ok(())
    }

    /// Loads media on the active media receiver.
    pub async fn load_media(
        &mut self,
        session_id: &str,
        url: &str,
        content_type: &str,
    ) -> Result<()> {
        let req_id = self.next_request_id();
        let load_req = MediaRequest::Load {
            request_id: req_id,
            media: MediaInfo {
                content_id: url.to_string(),
                content_type: content_type.to_string(),
                stream_type: "BUFFERED".to_string(), // or LIVE/NONE
            },
            autoplay: true,
        };

        let msg = CastMessage {
            protocol_version: cast_message::ProtocolVersion::Castv210 as i32,
            source_id: SENDER_ID.to_string(),
            destination_id: session_id.to_string(), // Send to transport_id
            namespace: "urn:x-cast:com.google.cast.media".to_string(),
            payload_type: cast_message::PayloadType::String as i32,
            payload_utf8: Some(serde_json::to_string(&load_req)?),
            payload_binary: None,
        };
        self.transport.write_message(&msg).await?;
        Ok(())
    }

    /// Controls transport (play, pause, stop).
    pub async fn transport(
        &mut self,
        session_id: &str,
        media_session_id: i32,
        cmd: &str,
    ) -> Result<()> {
        let req_id = self.next_request_id();
        let req = match cmd {
            "play" => MediaRequest::Play {
                request_id: req_id,
                media_session_id,
            },
            "pause" => MediaRequest::Pause {
                request_id: req_id,
                media_session_id,
            },
            "stop" => MediaRequest::Stop {
                request_id: req_id,
                media_session_id,
            },
            _ => return Ok(()),
        };

        let msg = CastMessage {
            protocol_version: cast_message::ProtocolVersion::Castv210 as i32,
            source_id: SENDER_ID.to_string(),
            destination_id: session_id.to_string(),
            namespace: "urn:x-cast:com.google.cast.media".to_string(),
            payload_type: cast_message::PayloadType::String as i32,
            payload_utf8: Some(serde_json::to_string(&req)?),
            payload_binary: None,
        };
        self.transport.write_message(&msg).await?;
        Ok(())
    }

    /// Sets the volume (0.0 to 1.0).
    pub async fn set_volume(&mut self, level: f64) -> Result<()> {
        let req_id = self.next_request_id();
        let vol_req = ReceiverRequest::SetVolume {
            request_id: req_id,
            volume: Volume {
                level: Some(level),
                muted: None,
            },
        };

        let msg = CastMessage {
            protocol_version: cast_message::ProtocolVersion::Castv210 as i32,
            source_id: SENDER_ID.to_string(),
            destination_id: DEFAULT_RECEIVER_ID.to_string(),
            namespace: "urn:x-cast:com.google.cast.receiver".to_string(),
            payload_type: cast_message::PayloadType::String as i32,
            payload_utf8: Some(serde_json::to_string(&vol_req)?),
            payload_binary: None,
        };
        self.transport.write_message(&msg).await?;
        Ok(())
    }

    /// Stops the given receiver session.
    pub async fn stop(&mut self, session_id: &str) -> Result<()> {
        let req_id = self.next_request_id();
        let stop_req = ReceiverRequest::Stop {
            request_id: req_id,
            session_id: session_id.to_string(),
        };

        let msg = CastMessage {
            protocol_version: cast_message::ProtocolVersion::Castv210 as i32,
            source_id: SENDER_ID.to_string(),
            destination_id: DEFAULT_RECEIVER_ID.to_string(),
            namespace: "urn:x-cast:com.google.cast.receiver".to_string(),
            payload_type: cast_message::PayloadType::String as i32,
            payload_utf8: Some(serde_json::to_string(&stop_req)?),
            payload_binary: None,
        };
        self.transport.write_message(&msg).await?;
        Ok(())
    }
}
