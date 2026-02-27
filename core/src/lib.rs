//! openclipboard_core
//!
//! Rust core for openclipboard: identity, framing, and transport primitives.
//! v0 focus: frame codec + message types + QUIC wiring stubs.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 0;

/// Logical stream IDs (application-level multiplexing).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamId {
    Control = 1,
    Clipboard = 2,
    File = 3,
}

/// Message types (application-level).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgType {
    Hello = 1,
    Ping = 2,
    Pong = 3,

    ClipText = 10,
    ClipImage = 11,

    FileOffer = 20,
    FileAccept = 21,
    FileReject = 22,
    FileChunk = 23,
    FileDone = 24,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipText {
    pub mime: String, // "text/plain"
    pub text: String,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipImage {
    pub mime: String, // "image/png"
    pub width: u32,
    pub height: u32,
    /// Base64-encoded bytes (v0). Later: binary chunk stream.
    pub bytes_b64: String,
    pub ts_ms: u64,
}

/// Frame header + payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub version: u8,
    pub msg_type: u8,
    pub stream_id: u32,
    pub seq: u64,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(msg_type: MsgType, stream_id: StreamId, seq: u64, payload: Vec<u8>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            msg_type: msg_type as u8,
            stream_id: stream_id as u32,
            seq,
            payload,
        }
    }
}

/// Encode a frame to bytes.
pub fn encode_frame(f: &Frame) -> Vec<u8> {
    use bytes::{BufMut, BytesMut};
    let mut b = BytesMut::with_capacity(1 + 1 + 4 + 8 + 4 + f.payload.len());
    b.put_u8(f.version);
    b.put_u8(f.msg_type);
    b.put_u32(f.stream_id);
    b.put_u64(f.seq);
    b.put_u32(f.payload.len() as u32);
    b.extend_from_slice(&f.payload);
    b.to_vec()
}

/// Decode a frame from bytes. Returns error if insufficient data.
pub fn decode_frame(mut bytes: &[u8]) -> anyhow::Result<Frame> {
    use bytes::Buf;
    if bytes.len() < 1 + 1 + 4 + 8 + 4 {
        anyhow::bail!("insufficient data");
    }
    let version = bytes.get_u8();
    let msg_type = bytes.get_u8();
    let stream_id = bytes.get_u32();
    let seq = bytes.get_u64();
    let len = bytes.get_u32() as usize;
    if bytes.len() < len {
        anyhow::bail!("payload truncated");
    }
    let payload = bytes[..len].to_vec();
    Ok(Frame {
        version,
        msg_type,
        stream_id,
        seq,
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_frame() {
        let f = Frame::new(MsgType::Ping, StreamId::Control, 42, b"hi".to_vec());
        let enc = encode_frame(&f);
        let dec = decode_frame(&enc).unwrap();
        assert_eq!(dec, f);
    }
}
