use crate::error::{AtrError, Result};
use crate::keys::{key_inject, Direction, Key};
use crate::proto::remote::{
    RemoteAppInfo, RemoteAppLinkLaunchRequest, RemoteImeKeyInject, RemoteMessage,
    RemotePingRequest, RemoteSetVolumeLevel, RemoteStart,
};
use prost::Message;
use std::io::{Read, Write};

/// A parsed response from the control port.
#[derive(Debug, Clone)]
pub enum ControlResponse {
    /// A volume read/set response.
    Volume(RemoteSetVolumeLevel),
    /// An app info response.
    AppInfo(RemoteAppInfo),
    /// A start/stop response.
    Start(RemoteStart),
    /// A key inject response (usually empty).
    KeyInject,
    /// A ping response.
    Ping,
    /// An error response.
    Error(String),
    /// An unrecognized response.
    Unknown,
}

/// Encode a `RemoteMessage` with a varint length prefix.
fn encode_frame(msg: &RemoteMessage) -> Vec<u8> {
    let mut buf = Vec::new();
    let len = msg.encoded_len();
    crate::pairing::encode_varint(len as u64, &mut buf);
    msg.encode(&mut buf)
        .expect("prost encode into Vec cannot fail");
    buf
}

/// Read one varint-framed `RemoteMessage` from the stream.
fn read_frame<S: Read>(stream: &mut S) -> Result<RemoteMessage> {
    let mut len_buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte).map_err(AtrError::Tls)?;
        len_buf.push(byte[0]);
        if byte[0] & 0x80 == 0 {
            break;
        }
    }
    let mut len: u64 = 0;
    let mut shift = 0;
    for &b in &len_buf {
        len |= ((b & 0x7F) as u64) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    let mut msg_buf = vec![0u8; len as usize];
    stream.read_exact(&mut msg_buf).map_err(AtrError::Tls)?;
    RemoteMessage::decode(&msg_buf[..]).map_err(AtrError::Decode)
}

/// A control session over the protobuf port (6466).
pub struct ControlSession<S> {
    stream: S,
}

impl<S> ControlSession<S>
where
    S: Read + Write,
{
    pub fn new(stream: S) -> Self {
        Self { stream }
    }

    pub fn send_raw(&mut self, msg: RemoteMessage) -> Result<RemoteMessage> {
        let frame = encode_frame(&msg);
        self.stream.write_all(&frame).map_err(AtrError::Tls)?;
        self.stream.flush().map_err(AtrError::Tls)?;
        read_frame(&mut self.stream)
    }

    pub fn press_key(&mut self, key: Key) -> Result<ControlResponse> {
        self.send_key(key, Direction::Short)
    }

    pub fn send_key(&mut self, key: Key, direction: Direction) -> Result<ControlResponse> {
        let inject = key_inject(key, direction);
        let msg = RemoteMessage {
            remote_key_inject: Some(inject),
            ..Default::default()
        };
        let _resp = self.send_raw(msg)?;
        Ok(ControlResponse::KeyInject)
    }

    pub fn get_volume(&mut self) -> Result<ControlResponse> {
        let msg = RemoteMessage {
            remote_set_volume_level: Some(RemoteSetVolumeLevel::default()),
            ..Default::default()
        };
        let resp = self.send_raw(msg)?;
        if let Some(vol) = resp.remote_set_volume_level {
            Ok(ControlResponse::Volume(vol))
        } else {
            Ok(ControlResponse::Unknown)
        }
    }

    pub fn set_volume(&mut self, level: u32) -> Result<ControlResponse> {
        let msg = RemoteMessage {
            remote_set_volume_level: Some(RemoteSetVolumeLevel {
                volume_level: level,
                ..Default::default()
            }),
            ..Default::default()
        };
        let resp = self.send_raw(msg)?;
        if let Some(vol) = resp.remote_set_volume_level {
            Ok(ControlResponse::Volume(vol))
        } else {
            Ok(ControlResponse::Unknown)
        }
    }

    pub fn get_app_info(&mut self) -> Result<ControlResponse> {
        let msg = RemoteMessage {
            remote_ime_key_inject: Some(RemoteImeKeyInject {
                app_info: Some(RemoteAppInfo::default()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let resp = self.send_raw(msg)?;
        if let Some(info) = resp
            .remote_ime_key_inject
            .as_ref()
            .and_then(|i| i.app_info.as_ref())
        {
            Ok(ControlResponse::AppInfo(info.clone()))
        } else {
            Ok(ControlResponse::Unknown)
        }
    }

    pub fn launch_app_link(&mut self, app_link: &str) -> Result<ControlResponse> {
        let msg = RemoteMessage {
            remote_app_link_launch_request: Some(RemoteAppLinkLaunchRequest {
                app_link: app_link.into(),
            }),
            ..Default::default()
        };
        let _resp = self.send_raw(msg)?;
        Ok(ControlResponse::Unknown)
    }

    pub fn start(&mut self, started: bool) -> Result<ControlResponse> {
        let msg = RemoteMessage {
            remote_start: Some(RemoteStart { started }),
            ..Default::default()
        };
        let resp = self.send_raw(msg)?;
        if let Some(start) = resp.remote_start {
            Ok(ControlResponse::Start(start))
        } else {
            Ok(ControlResponse::Unknown)
        }
    }

    pub fn ping(&mut self) -> Result<ControlResponse> {
        let msg = RemoteMessage {
            remote_ping_request: Some(RemotePingRequest { val1: 0, val2: 0 }),
            ..Default::default()
        };
        let resp = self.send_raw(msg)?;
        if resp.remote_ping_response.is_some() {
            Ok(ControlResponse::Ping)
        } else {
            Ok(ControlResponse::Unknown)
        }
    }
}
